use console::Term;
use playpen_content::{ContentBlock, Event, StopReason};
use std::io::Write;

pub struct Printer {
    role: String,
    buf: String,
    term: Term,
    displayed_temp: String,
}

impl Printer {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            role: String::new(),
            term: Term::stdout(),
            displayed_temp: String::new(),
        }
    }

    pub fn print(&mut self, event: &Event) {
        match event {
            Event::UserMessage { content, .. } => {
                if let Some(text) = text_from_blocks(content) {
                    self.push("user", text.trim());
                    self.commit();
                }
            }
            Event::ModelMessageDelta { text, .. } => {
                self.push("model", text);
            }
            Event::ModelMessage { .. } => {
                self.commit();
            }
            Event::ModelThoughtDelta { text, .. } => self.push("thinking", text),
            Event::ModelThought { .. } => self.commit(),
            Event::FunctionCall { name, args, .. } => {
                self.push(
                    name,
                    &format!("{}\n", serde_json::to_string(args).unwrap_or_default()),
                );
            }
            Event::FunctionOutputDelta { name, text, .. } => self.push(name, text),
            Event::FunctionResult { name, .. } => {
                self.push(name, "done\n");
                self.commit();
            }
            Event::TurnStop { stop_reason, .. } => {
                if matches!(stop_reason, StopReason::Cancelled) {
                    self.push("user", "[Cancelled]");
                }
            }
            Event::StateUpdate { .. } => {}
        }
    }

    pub fn commit(&mut self) {
        if !self.buf.is_empty() {
            let text = std::mem::take(&mut self.buf);
            if !self.displayed_temp.is_empty() {
                let _ = self.term.clear_line();
            }

            if !self.role.is_empty() {
                let role = self.role.as_str();
                let _ = self.term.write_line(&format!("[{role}] {text}"));
            } else {
                let _ = self.term.write_line(&text);
            }
            self.displayed_temp.clear();
        }
    }

    fn push(&mut self, role: &str, text: &str) {
        self.role = role.to_string();
        self.buf.push_str(text);

        while let Some(pos) = self.buf.find('\n') {
            let mut line = self.buf.drain(..=pos).collect::<String>();
            line.pop();

            if !self.displayed_temp.is_empty() {
                let _ = self.term.clear_line();
                self.displayed_temp.clear();
            }
            let _ = self.term.write_line(&format!("[{role}] {line}"));
        }

        if !self.buf.is_empty() && self.buf != self.displayed_temp {
            let _ = self.term.clear_line();
            let _ = write!(self.term, "{}", self.buf);
            let _ = self.term.flush();
            self.displayed_temp = self.buf.clone();
        }
    }
}

impl Drop for Printer {
    fn drop(&mut self) {
        self.commit();
    }
}

fn text_from_blocks(blocks: &[ContentBlock]) -> Option<String> {
    blocks.iter().find_map(|b| match b {
        ContentBlock::Text(t) => Some(t.text.clone()),
        _ => None,
    })
}
