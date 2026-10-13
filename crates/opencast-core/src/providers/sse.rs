//! Incremental Server-Sent Events parser.
//!
//! Fed raw byte chunks as they arrive (which may split lines or even UTF-8
//! characters), it yields complete events.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            self.line(&line, &mut out);
        }
        out
    }

    /// Flush a final event if the stream ended without a blank line.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buf.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buf)).into_owned();
            let mut out = Vec::new();
            self.line(rest.trim_end_matches('\r'), &mut out);
        }
        self.dispatch()
    }

    fn line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            if let Some(ev) = self.dispatch() {
                out.push(ev);
            }
            return;
        }
        if line.starts_with(':') {
            return; // comment / keep-alive
        }
        let (field, value) = match line.find(':') {
            Some(i) => {
                let v = &line[i + 1..];
                (&line[..i], v.strip_prefix(' ').unwrap_or(v))
            }
            None => (line, ""),
        };
        match field {
            "data" => self.data.push(value.to_string()),
            "event" => self.event = Some(value.to_string()),
            _ => {}
        }
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() {
            self.event = None;
            return None;
        }
        let ev = SseEvent {
            event: self.event.take(),
            data: self.data.join("\n"),
        };
        self.data.clear();
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_across_chunks_and_crlf() {
        let mut p = SseParser::new();
        assert!(p.feed(b"event: delta\r\nda").is_empty());
        let evs = p.feed(b"ta: {\"a\":1}\r\n\r\n: ping\n\ndata: x\n");
        assert_eq!(
            evs,
            vec![SseEvent {
                event: Some("delta".into()),
                data: "{\"a\":1}".into()
            }]
        );
        assert_eq!(
            p.finish(),
            Some(SseEvent {
                event: None,
                data: "x".into()
            })
        );
    }

    #[test]
    fn multiline_data_and_split_utf8() {
        let mut p = SseParser::new();
        let bytes = "data: héllo\ndata: wörld\n\n".as_bytes();
        let (a, b) = bytes.split_at(8); // splits inside 'é'
        assert!(p.feed(a).is_empty());
        let evs = p.feed(b);
        assert_eq!(evs[0].data, "héllo\nwörld");
    }
}
