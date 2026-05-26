    // CLI dispatch (generate/new/design) tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::path::PathBuf;

    use clap::Parser;

    use crate::cli_args::{DesignExportTarget, DesignImportFormat};
    use crate::{Cli, Commands, DesignCommand, GenerateKind};

    #[test]
    fn wave2_cli_dispatch_parses_new_surfaces() {
        let cli = Cli::try_parse_from(["lazuli", "generate", "feature", "billing"]).unwrap();
        let Commands::Generate {
            kind: GenerateKind::Feature,
            input,
            ..
        } = cli.command
        else {
            panic!("expected generate feature command");
        };
        assert_eq!(input, PathBuf::from("billing"));

        let cli =
            Cli::try_parse_from(["lazuli", "new", "demo", "--frontends", "web,mobile"]).unwrap();
        let Commands::New {
            frontends: Some(frontends),
            ..
        } = cli.command
        else {
            panic!("expected new command with frontends");
        };
        assert_eq!(frontends, "web,mobile");

        let cli =
            Cli::try_parse_from(["lazuli", "new", "--frontends", "web", "--in-place"]).unwrap();
        let Commands::New {
            project_name: None,
            frontends: Some(frontends),
            in_place: true,
            ..
        } = cli.command
        else {
            panic!("expected in-place new command without project name");
        };
        assert_eq!(frontends, "web");

        let cli = Cli::try_parse_from([
            "lazuli",
            "design",
            "import",
            "--from",
            "tokens.figma.json",
            "--format",
            "figma",
            "--overwrite",
        ])
        .unwrap();
        let Commands::Design {
            sub:
                DesignCommand::Import {
                    format: DesignImportFormat::Figma,
                    overwrite: true,
                    ..
                },
        } = cli.command
        else {
            panic!("expected design import command");
        };

        let cli = Cli::try_parse_from([
            "lazuli",
            "design",
            "export",
            "--target",
            "style-dictionary",
            "--out",
            "tokens.sd.json",
        ])
        .unwrap();
        let Commands::Design {
            sub:
                DesignCommand::Export {
                    target: DesignExportTarget::StyleDictionary,
                    ..
                },
        } = cli.command
        else {
            panic!("expected design export command");
        };

        let cli = Cli::try_parse_from(["lazuli", "design", "diff", "--against", "tokens.sd.json"])
            .unwrap();
        let Commands::Design {
            sub: DesignCommand::Diff { against },
        } = cli.command
        else {
            panic!("expected design diff command");
        };
        assert_eq!(against, PathBuf::from("tokens.sd.json"));
    }
