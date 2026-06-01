use crate::events::Events;
use crate::state::State;

/// Session 只读视图。
pub trait Session: Send + Sync {
    fn id(&self) -> &str;
    fn state(&self) -> &dyn State;
    fn events(&self) -> &dyn Events;
}
