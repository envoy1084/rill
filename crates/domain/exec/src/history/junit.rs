//! Test cases from a JUnit XML report (`quick-junit`).

use quick_junit::{Report, TestCaseStatus};

use super::outcome::TestOutcome;

/// One test case of a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseResult {
    /// `<classname>::<name>`, or the name when there is no class.
    pub id: String,
    pub outcome: TestOutcome,
    /// The runner retried it before it passed.
    pub retried: bool,
    pub duration_ms: Option<u64>,
}

/// The test cases of the JUnit XML `xml`; `None` when it is not a report.
pub fn parse_junit(xml: &str) -> Option<Vec<TestCaseResult>> {
    let report = Report::deserialize_from_str(xml).ok()?;
    let mut out = Vec::new();
    for suite in &report.test_suites {
        for case in &suite.test_cases {
            let (outcome, retried) = match &case.status {
                TestCaseStatus::Success { flaky_runs } => {
                    (TestOutcome::Passed, !flaky_runs.is_empty())
                }
                TestCaseStatus::NonSuccess { .. } => (TestOutcome::Failed, false),
                TestCaseStatus::Skipped { .. } => (TestOutcome::Skipped, false),
            };
            let id = match &case.classname {
                Some(class) if !class.is_empty() => {
                    format!("{}::{}", class.as_str(), case.name.as_str())
                }
                _ => case.name.as_str().to_string(),
            };
            out.push(TestCaseResult {
                id,
                outcome,
                retried,
                duration_ms: case.time.map(|t| t.as_millis() as u64),
            });
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_outcomes_retries_and_durations() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" failures="1" errors="0">
  <testsuite name="core" tests="3" disabled="0" errors="0" failures="1">
    <testcase name="merge::clean" classname="core" time="0.120"/>
    <testcase name="merge::racy" classname="core" time="0.300">
      <flakyFailure type="test failure" message="timed out"/>
    </testcase>
    <testcase name="diff::empty" classname="core" time="0.010">
      <failure type="test failure" message="assertion failed"/>
    </testcase>
    <testcase name="slow" classname="core"><skipped/></testcase>
  </testsuite>
</testsuites>"#;
        let cases = parse_junit(xml).unwrap();
        assert_eq!(cases.len(), 4);
        assert_eq!(cases[0].id, "core::merge::clean");
        assert_eq!(cases[0].duration_ms, Some(120));
        assert!(cases[1].retried && cases[1].outcome == TestOutcome::Passed);
        assert_eq!(cases[2].outcome, TestOutcome::Failed);
        assert_eq!(cases[3].outcome, TestOutcome::Skipped);
        assert!(parse_junit("not xml").is_none());
    }
}
