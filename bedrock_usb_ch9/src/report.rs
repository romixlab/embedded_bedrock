//! A flat pass/fail/skip log for one run of chapter 9 checks, independent of how it is printed so the counting
//! and the exit code stay unit-testable without a terminal or a device.

#[derive(Debug)]
pub enum Outcome {
    Pass,
    Fail(String),
    Skip(String),
}

#[derive(Debug)]
pub struct Entry {
    pub name: String,
    /// A rule id from design/bedrock-usb.md (R1..R14) or the check catalogue, when the check is about one.
    pub rule: Option<&'static str>,
    pub outcome: Outcome,
}

#[derive(Debug, Default)]
pub struct Report {
    pub entries: Vec<Entry>,
}

impl Report {
    pub fn pass(&mut self, name: impl Into<String>, rule: Option<&'static str>) {
        self.entries.push(Entry {
            name: name.into(),
            rule,
            outcome: Outcome::Pass,
        });
    }

    pub fn fail(
        &mut self,
        name: impl Into<String>,
        rule: Option<&'static str>,
        why: impl Into<String>,
    ) {
        self.entries.push(Entry {
            name: name.into(),
            rule,
            outcome: Outcome::Fail(why.into()),
        });
    }

    pub fn skip(
        &mut self,
        name: impl Into<String>,
        rule: Option<&'static str>,
        why: impl Into<String>,
    ) {
        self.entries.push(Entry {
            name: name.into(),
            rule,
            outcome: Outcome::Skip(why.into()),
        });
    }

    pub fn passed(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.outcome, Outcome::Pass))
            .count()
    }

    pub fn failed(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.outcome, Outcome::Fail(_)))
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.outcome, Outcome::Skip(_)))
            .count()
    }

    /// 0 when nothing failed (skips don't count against it: a device missing a feature, like alternate
    /// settings, isn't a chapter 9 violation), 1 otherwise.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_outcome_once() {
        let mut r = Report::default();
        r.pass("a", None);
        r.fail("b", Some("R12"), "went wrong");
        r.skip("c", None, "not applicable");
        r.pass("d", Some("R13"));
        assert_eq!(r.passed(), 2);
        assert_eq!(r.failed(), 1);
        assert_eq!(r.skipped(), 1);
        assert_eq!(r.exit_code(), 1);
    }

    #[test]
    fn exit_code_is_clean_without_a_failure() {
        let mut r = Report::default();
        r.pass("a", None);
        r.skip("b", None, "no bulk endpoint");
        assert_eq!(r.exit_code(), 0);
    }

    #[test]
    fn empty_report_is_clean() {
        assert_eq!(Report::default().exit_code(), 0);
    }
}
