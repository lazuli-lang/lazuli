
    use super::*;

    #[test]
    fn layer_round_trips_through_str() {
        for layer in Layer::ALL {
            assert_eq!(Layer::parse(layer.as_str()), Some(*layer));
        }
        assert_eq!(Layer::parse("nope"), None);
    }

    #[test]
    fn fail_on_parses_category() {
        let spec = FailOnSpec::parse("category:TestDiscipline").unwrap();
        assert_eq!(spec, FailOnSpec::Category("TestDiscipline".into()));
    }

    #[test]
    fn fail_on_parses_coverage_metric() {
        let spec = FailOnSpec::parse("coverage:handler_go=70").unwrap();
        assert_eq!(
            spec,
            FailOnSpec::Coverage {
                metric: "handler_go".into(),
                threshold: 70.0
            }
        );
    }

    #[test]
    fn fail_on_parses_coverage_aggregate() {
        let spec = FailOnSpec::parse("coverage:aggregate=85").unwrap();
        assert_eq!(
            spec,
            FailOnSpec::Coverage {
                metric: "aggregate".into(),
                threshold: 85.0
            }
        );
    }

    #[test]
    fn fail_on_rejects_unknown_prefix() {
        assert!(FailOnSpec::parse("severity:error").is_err());
    }

    #[test]
    fn fail_on_rejects_missing_threshold() {
        assert!(FailOnSpec::parse("coverage:handler_go").is_err());
    }

    #[test]
    fn fail_on_rejects_non_numeric_threshold() {
        assert!(FailOnSpec::parse("coverage:handler_go=high").is_err());
    }

    #[test]
    fn fail_on_rejects_out_of_range() {
        assert!(FailOnSpec::parse("coverage:handler_go=150").is_err());
        assert!(FailOnSpec::parse("coverage:handler_go=-1").is_err());
    }

    #[test]
    fn report_aggregation_picks_fail_when_any_layer_failed() {
        let acc = RunAccumulator {
            layer_results: vec![
                LayerResult {
                    layer: Layer::Spec,
                    runner: "lazuli-doctor".into(),
                    result: LayerVerdict::Pass,
                    tests_run: 1,
                    tests_passed: 1,
                    tests_failed: 0,
                    issues: 0,
                    exit_code: Some(0),
                    command: None,
                    duration_ms: 10,
                    failures: vec![],
                    runner_native_only: None,
                    skip_reason: None,
                },
                LayerResult {
                    layer: Layer::Handler,
                    runner: "go-test".into(),
                    result: LayerVerdict::Fail,
                    tests_run: 5,
                    tests_passed: 4,
                    tests_failed: 1,
                    issues: 0,
                    exit_code: Some(1),
                    command: Some("go test ./...".into()),
                    duration_ms: 100,
                    failures: vec![],
                    runner_native_only: None,
                    skip_reason: None,
                },
            ],
            ..Default::default()
        };
        let report = acc.finalize(120);
        assert_eq!(report.summary.layers_run, 2);
        assert_eq!(report.summary.layers_failed, 1);
        assert_eq!(report.summary.overall, LayerVerdict::Fail);
    }

    #[test]
    fn report_aggregation_picks_pass_when_all_layers_passed() {
        let acc = RunAccumulator {
            layer_results: vec![LayerResult {
                layer: Layer::Spec,
                runner: "lazuli-doctor".into(),
                result: LayerVerdict::Pass,
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                issues: 0,
                exit_code: None,
                command: None,
                duration_ms: 5,
                failures: vec![],
                runner_native_only: None,
                skip_reason: None,
            }],
            ..Default::default()
        };
        let report = acc.finalize(10);
        assert_eq!(report.summary.overall, LayerVerdict::Pass);
    }
