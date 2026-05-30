
    use super::*;

    #[test]
    fn parses_vitest_jest_shared_shape() {
        let json = br#"{
          "numTotalTests": 3,
          "numPassedTests": 2,
          "numFailedTests": 1,
          "testResults": [
            { "name": "src/post.test.ts",
              "assertionResults": [
                { "title": "creates draft", "status": "passed", "duration": 12 },
                { "title": "publishes", "fullName": "post > publishes",
                  "status": "failed", "duration": 30,
                  "failureMessages": ["AssertionError: expected 1 to be 2"] }
              ] }
          ]
        }"#;
        let parsed = parse_vitest_json(json);
        assert_eq!(parsed.tests_run, 3);
        assert_eq!(parsed.tests_passed, 2);
        assert_eq!(parsed.tests_failed, 1);
        assert_eq!(parsed.failures.len(), 1);
        assert_eq!(parsed.failures[0].test, "post > publishes");
        assert_eq!(parsed.failures[0].duration_ms, Some(30));
        assert!(parsed.failures[0].message.contains("expected 1 to be 2"));
    }

    #[test]
    fn recounts_from_assertions_when_totals_missing() {
        let json = br#"{
          "testResults": [
            { "name": "a", "assertionResults": [
                { "title": "t1", "status": "passed" },
                { "title": "t2", "status": "passed" }
            ] },
            { "name": "b", "assertionResults": [
                { "title": "t3", "status": "failed",
                  "failureMessages": ["nope"] }
            ] }
          ]
        }"#;
        let parsed = parse_jest_json(json);
        assert_eq!(parsed.tests_run, 3);
        assert_eq!(parsed.tests_passed, 2);
        assert_eq!(parsed.tests_failed, 1);
    }

    #[test]
    fn handles_leading_garbage_before_json() {
        let mut bytes = b"\x1b[2J".to_vec();
        bytes.extend_from_slice(
            br#"{"numTotalTests":1,"numPassedTests":1,"numFailedTests":0,"testResults":[]}"#,
        );
        bytes.push(b'\n');
        let parsed = parse_vitest_json(&bytes);
        assert_eq!(parsed.tests_run, 1);
        assert_eq!(parsed.tests_passed, 1);
    }

    #[test]
    fn rejects_unknown_runner() {
        assert!(TsRunner::parse("mocha").is_err());
        assert!(TsRunner::parse("vitest").is_ok());
        assert!(TsRunner::parse("jest").is_ok());
    }

    #[test]
    fn empty_input_yields_zero() {
        let parsed = parse_vitest_json(b"");
        assert_eq!(parsed.tests_run, 0);
    }
