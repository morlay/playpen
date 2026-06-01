pub(crate) mod convert;
pub mod entities;
pub(crate) mod migration;

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use sea_orm_migration::MigratorTrait;
use serde_json::Value;

use self::entities::{events, session_events, sessions};
use crate::events::Events;
use crate::role::Role;
use crate::service::SessionService;
use crate::session::Session;
use crate::state::State;
use playpen_content::Event;

// ── Concrete Session — 实时查 DB ────────────────────────────────────

struct DbSession {
    id: String,
    db: Arc<DatabaseConnection>,
    role_filter: Option<Role>,
}

impl DbSession {
    fn new(id: String, db: Arc<DatabaseConnection>) -> Self {
        Self {
            id,
            db,
            role_filter: None,
        }
    }
}

impl Session for DbSession {
    fn id(&self) -> &str {
        &self.id
    }
    fn state(&self) -> &dyn State {
        self
    }
    fn events(&self) -> &dyn Events {
        self
    }
}

#[async_trait]
impl State for DbSession {
    async fn get(&self, key: &str) -> Option<Value> {
        // 子查询：先从小表 session_events 过滤 session_id，再查 events
        use sea_orm::sea_query::{Expr, Query};
        let sub = Query::select()
            .column(session_events::Column::FEventId)
            .from(session_events::Entity)
            .and_where(Expr::col(session_events::Column::FSessionId).eq(self.id.clone()))
            .to_owned();

        // ORDER BY created_at DESC 取最新值
        let event_row = events::Entity::find()
            .filter(events::Column::FKind.eq("state_update"))
            .filter(events::Column::FRole.eq("state"))
            .filter(events::Column::FName.eq(key))
            .filter(events::Column::FEventId.in_subquery(sub))
            .order_by_desc(events::Column::FCreatedAt)
            .one(&*self.db)
            .await
            .ok()??;

        convert::decode_state_data::<Value>(&event_row.f_data).ok()
    }

    async fn set(&mut self, key: String, value: Value) {
        // 值未变化，跳过
        if self.get(&key).await.as_ref() == Some(&value) {
            return;
        }

        let eid = uuid::Uuid::now_v7().to_string();
        let _ = insert_event_row(
            &self.db,
            &self.id,
            &eid,
            "state_update",
            "state",
            Some(key),
            "",
            &value,
        )
        .await;
    }

    async fn entities(&self) -> BoxStream<'_, (String, Value)> {
        // 子查询先过滤 session_id，再 GROUP BY name 取唯一 key
        use sea_orm::sea_query::{Expr, Query};
        let sub = Query::select()
            .column(session_events::Column::FEventId)
            .from(session_events::Entity)
            .and_where(Expr::col(session_events::Column::FSessionId).eq(self.id.clone()))
            .to_owned();

        let stream = events::Entity::find()
            .filter(events::Column::FRole.eq("state"))
            .filter(events::Column::FKind.eq("state_update"))
            .filter(events::Column::FEventId.in_subquery(sub))
            .group_by(events::Column::FName)
            .order_by_desc(events::Column::FCreatedAt)
            .stream(&*self.db)
            .await;

        match stream {
            Ok(stream) => Box::pin(stream.filter_map(|result| async move {
                match result {
                    Ok(row) => convert::decode_state_data::<Value>(&row.f_data)
                        .ok()
                        .map(|v| (row.f_name, v)),
                    Err(_) => None,
                }
            })),
            Err(_) => Box::pin(futures::stream::empty()),
        }
    }
}

#[async_trait]
impl Events for DbSession {
    async fn append(&self, event: &Event) -> anyhow::Result<Event> {
        let normal = match convert::normalize_event(event) {
            Some(n) => n,
            None => return Ok(event.clone()),
        };

        // 先生成 event_id，确保 payload.id 注入使用同一 ID
        let event_id = uuid::Uuid::now_v7().to_string();

        // state_update 的 payload 就是值本身，无需 id 注入
        let payload = if normal.kind == "state_update" {
            normal.payload
        } else {
            let mut p = normal.payload;
            if p.get("id").map(|v| v.is_null()).unwrap_or(true) {
                p["id"] = serde_json::json!(event_id);
            }
            p
        };

        insert_event_row(
            &self.db,
            &self.id,
            &event_id,
            normal.kind,
            normal.role,
            normal.name,
            &normal.action_id,
            &payload,
        )
        .await?;

        Ok(event.clone().with_id(event_id))
    }

    async fn all(&self) -> BoxStream<'_, Event> {
        use sea_orm::sea_query::{Expr, Order, Query};
        // 子查询先从小表取 event_id + sequence
        let sub = Query::select()
            .column(session_events::Column::FEventId)
            .from(session_events::Entity)
            .and_where(Expr::col(session_events::Column::FSessionId).eq(self.id.clone()))
            .order_by(session_events::Column::FSequence, Order::Asc)
            .to_owned();

        let mut q = events::Entity::find()
            .filter(events::Column::FEventId.in_subquery(sub))
            .order_by_asc(events::Column::FId);

        if let Some(role) = &self.role_filter {
            q = q.filter(events::Column::FRole.eq(role.as_str()));
        }

        let stream = q.stream(&*self.db).await;

        match stream {
            Ok(stream) => Box::pin(stream.filter_map(move |result| async move {
                match result {
                    Ok(row) => {
                        let mut events = convert::denormalize_events(
                            &row.f_kind,
                            &row.f_role,
                            if row.f_name.is_empty() {
                                None
                            } else {
                                Some(row.f_name.as_str())
                            },
                            &row.f_data,
                            &row.f_event_id,
                        );
                        events.pop()
                    }
                    Err(_) => None,
                }
            })),
            Err(_) => Box::pin(futures::stream::empty()),
        }
    }

    async fn len(&self) -> usize {
        match session_events::Entity::find()
            .filter(session_events::Column::FSessionId.eq(&self.id))
            .count(&*self.db)
            .await
        {
            Ok(n) => n as usize,
            Err(_) => 0,
        }
    }

    fn by_role(&self, roles: &[Role]) -> Box<dyn Events + '_> {
        Box::new(RoleFilteredEvents {
            inner: self,
            roles: roles.to_vec(),
        })
    }
}

// ── RoleFilteredEvents ──────────────────────────────────────────────

/// 带 role 过滤条件的 Events 视图。
struct RoleFilteredEvents<'a> {
    inner: &'a DbSession,
    roles: Vec<Role>,
}

#[async_trait]
impl Events for RoleFilteredEvents<'_> {
    async fn all(&self) -> BoxStream<'_, Event> {
        use sea_orm::sea_query::{Expr, Order, Query};
        let sub = Query::select()
            .column(session_events::Column::FEventId)
            .from(session_events::Entity)
            .and_where(Expr::col(session_events::Column::FSessionId).eq(self.inner.id.clone()))
            .order_by(session_events::Column::FSequence, Order::Asc)
            .to_owned();

        let role_strs: Vec<&str> = self.roles.iter().map(|r| r.as_str()).collect();

        let stream = events::Entity::find()
            .filter(events::Column::FEventId.in_subquery(sub))
            .filter(events::Column::FRole.is_in(role_strs))
            .order_by_asc(events::Column::FCreatedAt)
            .stream(&*self.inner.db)
            .await;

        match stream {
            Ok(stream) => Box::pin(stream.filter_map(move |result| async move {
                match result {
                    Ok(row) => {
                        let mut events = convert::denormalize_events(
                            &row.f_kind,
                            &row.f_role,
                            if row.f_name.is_empty() {
                                None
                            } else {
                                Some(row.f_name.as_str())
                            },
                            &row.f_data,
                            &row.f_event_id,
                        );
                        events.pop()
                    }
                    Err(_) => None,
                }
            })),
            Err(_) => Box::pin(futures::stream::empty()),
        }
    }

    async fn len(&self) -> usize {
        use sea_orm::sea_query::{Expr, Query};
        let sub = Query::select()
            .column(session_events::Column::FEventId)
            .from(session_events::Entity)
            .and_where(Expr::col(session_events::Column::FSessionId).eq(self.inner.id.clone()))
            .to_owned();

        let role_strs: Vec<&str> = self.roles.iter().map(|r| r.as_str()).collect();

        events::Entity::find()
            .filter(events::Column::FEventId.in_subquery(sub))
            .filter(events::Column::FRole.is_in(role_strs))
            .count(&*self.inner.db)
            .await
            .unwrap_or(0) as usize
    }

    async fn append(&self, event: &Event) -> anyhow::Result<Event> {
        self.inner.append(event).await
    }

    fn by_role(&self, roles: &[Role]) -> Box<dyn Events + '_> {
        Box::new(RoleFilteredEvents {
            inner: self.inner,
            roles: roles.to_vec(),
        })
    }
}

// ── DBSessionService ────────────────────────────────────────────────

pub struct DBSessionService {
    db: DatabaseConnection,
}

impl DBSessionService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        // SQLite WAL 模式：允许并发读写
        self.db
            .execute(sea_orm::Statement::from_string(
                self.db.get_database_backend(),
                "PRAGMA journal_mode=WAL;",
            ))
            .await?;

        self::migration::Migrator::up(&self.db, None).await?;

        Ok(())
    }
}

// ── 事件归一化 ──────────────────────────────────────────────────────

/// 内部：在事务中插入一条事件记录（events + session_events + 更新 head）。
/// event_id 由调用方生成，确保与 payload.id 一致。
#[allow(clippy::too_many_arguments)]
async fn insert_event_row(
    db: &DatabaseConnection,
    session_id: &str,
    event_id: &str,
    kind: &str,
    role: &str,
    name: Option<String>,
    action_id: &str,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    let json = serde_json::to_vec(payload)?;
    let compressed = zstd::encode_all(std::io::Cursor::new(json), 3)?;
    let now = chrono::Utc::now().timestamp_millis();

    let session_row = sessions::Entity::find()
        .filter(sessions::Column::FSessionId.eq(session_id))
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session {session_id} 不存在"))?;

    let session_f_id = session_row.f_id;
    let parent_f_event_id = session_row.f_head_event_id;
    let next_seq = session_row.f_head_sequence + 1;

    use sea_orm::TransactionTrait;
    db.transaction(|txn| {
        let parent_f_event_id = parent_f_event_id.clone();
        let sid = session_id.to_string();
        let eid = event_id.to_string();
        let kind = kind.to_string();
        let role = role.to_string();
        let name = name.unwrap_or_default();
        let action_id = action_id.to_string();
        let compressed = compressed.clone();
        Box::pin(async move {
            events::Entity::insert(events::ActiveModel {
                f_id: sea_orm::NotSet,
                f_event_id: Set(eid.clone()),
                f_parent_id: Set(parent_f_event_id),
                f_encoding: Set("zstd+json".to_string()),
                f_kind: Set(kind),
                f_role: Set(role),
                f_name: Set(name),
                f_action_id: Set(action_id),
                f_data: Set(compressed),
                f_created_at: Set(now),
            })
            .exec(txn)
            .await?;

            session_events::Entity::insert(session_events::ActiveModel {
                f_id: sea_orm::NotSet,
                f_session_id: Set(sid.clone()),
                f_event_id: Set(eid.clone()),
                f_sequence: Set(next_seq),
            })
            .exec(txn)
            .await?;

            sessions::Entity::update(sessions::ActiveModel {
                f_id: Set(session_f_id),
                f_session_id: Set(sid),
                f_head_event_id: Set(eid.clone()),
                f_head_sequence: Set(next_seq),
            })
            .exec(txn)
            .await?;

            Ok::<(), sea_orm::DbErr>(())
        })
    })
    .await?;

    Ok(())
}

async fn verify_session_exists(db: &DatabaseConnection, id: &str) -> anyhow::Result<()> {
    sessions::Entity::find()
        .filter(sessions::Column::FSessionId.eq(id))
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session {id} 不存在"))?;
    Ok(())
}

// ── SessionService impl ──────────────────────────────────────────────

#[async_trait]
impl SessionService for DBSessionService {
    async fn create(&self) -> anyhow::Result<Box<dyn Session>> {
        let id = uuid::Uuid::now_v7().to_string();

        sessions::Entity::insert(sessions::ActiveModel {
            f_id: sea_orm::NotSet,
            f_session_id: Set(id.clone()),
            f_head_event_id: Set(String::new()),
            f_head_sequence: Set(-1),
        })
        .exec(&self.db)
        .await?;

        Ok(Box::new(DbSession::new(
            id.to_string(),
            Arc::new(self.db.clone()),
        )))
    }

    async fn get(&self, id: &str) -> anyhow::Result<Box<dyn Session>> {
        verify_session_exists(&self.db, id).await?;

        Ok(Box::new(DbSession::new(
            id.to_string(),
            Arc::new(self.db.clone()),
        )))
    }

    async fn rewind(&self, event_id: &str) -> anyhow::Result<Box<dyn Session>> {
        let se_row = session_events::Entity::find()
            .filter(session_events::Column::FEventId.eq(event_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("event {event_id} 不在任何 session 中"))?;

        let session_f_id = se_row.f_session_id.clone();
        let seq = se_row.f_sequence;

        use sea_orm::TransactionTrait;
        self.db
            .transaction(|txn| {
                let session_f_id = session_f_id.clone();
                Box::pin(async move {
                    session_events::Entity::delete_many()
                        .filter(session_events::Column::FSessionId.eq(&session_f_id))
                        .filter(session_events::Column::FSequence.gte(seq))
                        .exec(txn)
                        .await?;

                    // 获取 session 的 f_id 用于 update
                    let session = sessions::Entity::find()
                        .filter(sessions::Column::FSessionId.eq(&session_f_id))
                        .one(txn)
                        .await?
                        .ok_or_else(|| sea_orm::DbErr::Custom("session not found".into()))?;

                    if seq == 0 {
                        sessions::Entity::update(sessions::ActiveModel {
                            f_id: Set(session.f_id),
                            f_session_id: Set(session_f_id),
                            f_head_event_id: Set(String::new()),
                            f_head_sequence: Set(-1),
                        })
                        .exec(txn)
                        .await?;
                    } else {
                        let prev = session_events::Entity::find()
                            .filter(session_events::Column::FSessionId.eq(&session_f_id))
                            .filter(session_events::Column::FSequence.eq(seq - 1))
                            .one(txn)
                            .await?
                            .ok_or_else(|| {
                                sea_orm::DbErr::Custom("rewind: 前一个 event 丢失".into())
                            })?;

                        sessions::Entity::update(sessions::ActiveModel {
                            f_id: Set(session.f_id),
                            f_session_id: Set(session_f_id),
                            f_head_event_id: Set(prev.f_event_id),
                            f_head_sequence: Set(seq - 1),
                        })
                        .exec(txn)
                        .await?;
                    }

                    Ok::<(), sea_orm::DbErr>(())
                })
            })
            .await?;

        Ok(Box::new(DbSession::new(
            session_f_id,
            Arc::new(self.db.clone()),
        )))
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        session_events::Entity::delete_many()
            .filter(session_events::Column::FSessionId.eq(id))
            .exec(&self.db)
            .await?;

        sessions::Entity::delete_many()
            .filter(sessions::Column::FSessionId.eq(id))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn list(
        &self,
        limit: Option<usize>,
        offset: usize,
    ) -> anyhow::Result<Vec<Box<dyn Session>>> {
        use sea_orm::EntityTrait;

        let mut query = sessions::Entity::find().order_by_desc(sessions::Column::FId);

        if let Some(l) = limit {
            query = query.limit(l as u64);
        }
        if offset > 0 {
            query = query.offset(offset as u64);
        }

        let rows = query.all(&self.db).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(self.get(&row.f_session_id).await?);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
