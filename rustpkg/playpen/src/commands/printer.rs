use console::Term;
use std::io::Write;

pub struct Printer {
    buf: String,
    term: Term,
    /// 记录当前已经在屏幕上显示的「未换行临时文本」
    displayed_temp: String,
}

impl Printer {
    /// 默认输出到标准输出 (stdout)
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            term: Term::stdout(),
            displayed_temp: String::new(),
        }
    }

    /// 输出到标准错误 (stderr)
    #[allow(dead_code)]
    pub fn new_stderr() -> Self {
        Self {
            buf: String::new(),
            term: Term::stderr(),
            displayed_temp: String::new(),
        }
    }

    pub fn push(&mut self, text: &str) {
        self.buf.push_str(text);

        // 1. 循环提取所有带有 \n 的完整行
        while let Some(pos) = self.buf.find('\n') {
            let mut line = self.buf.drain(..=pos).collect::<String>();
            line.pop(); // 移除末尾的 '\n'

            // 如果屏幕上有临时文本，利用 console 库安全擦除它
            if !self.displayed_temp.is_empty() {
                let _ = self.term.clear_line();
                self.displayed_temp.clear();
            }

            // 正式输出确定的一行
            let _ = self.term.write_line(&line);
        }

        // 2. 处理剩下的、没有换行的临时文本（如进度提示）
        if !self.buf.is_empty() {
            // 只有当新内容和旧内容不同时才刷新，防止控制台高频闪烁
            if self.buf != self.displayed_temp {
                let _ = self.term.clear_line();
                // write! 会保持在当前行，不会自动换行
                let _ = write!(self.term, "{}", self.buf);
                let _ = self.term.flush();

                // 同步更新状态
                self.displayed_temp = self.buf.clone();
            }
        } else if !self.displayed_temp.is_empty() {
            // 如果 buf 空了，但控制台还残留着之前的临时文本，将其清空
            let _ = self.term.clear_line();
            let _ = self.term.flush();
            self.displayed_temp.clear();
        }
    }

    /// 强行将当前缓冲区的所有内容作为一行输出（强制换行）
    pub fn commit(&mut self) {
        if !self.buf.is_empty() {
            let text = std::mem::take(&mut self.buf);
            if !self.displayed_temp.is_empty() {
                let _ = self.term.clear_line();
                self.displayed_temp.clear();
            }
            let _ = self.term.write_line(&text);
        }
    }

    /// 强行插队打印一行紧急日志
    pub fn emit(&mut self, text: &str) {
        // 先干净地擦除当前屏幕上的临时动态行
        if !self.displayed_temp.is_empty() {
            let _ = self.term.clear_line();
        }

        // 打印插队数据并换行
        let _ = self.term.write_line(text);

        // 关键：因为插队数据换行了，原来的临时动态行被“推”到了最新的一行
        // 我们在最新的一行把它重新绘制出来
        if !self.buf.is_empty() {
            let _ = write!(self.term, "{}", self.buf);
            let _ = self.term.flush();
            self.displayed_temp = self.buf.clone();
        } else {
            self.displayed_temp.clear();
        }
    }
}

impl Drop for Printer {
    fn drop(&mut self) {
        self.commit();
    }
}
