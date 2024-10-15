use cicero::commands::Cli;

pub static CARGO_BUNDLE_LICENSES: Cli = Cli::install_crate("cargo-bundle-licenses");

pub static CARGO_DENY: Cli = Cli::install_crate("cargo-deny");

pub static CARGO_SBOM: Cli = Cli::install_crate("cargo-sbom");

pub static CARGO_TARPAULIN: Cli = Cli::install_crate("cargo-tarpaulin");

pub static CROSS: Cli = Cli::install_crate("cross")
    .with_default_command(&|mut command| {
        command
            .arg("build")
            .arg("--release");
        command
    });

pub static DIESEL: Cli = Cli::install_crate_with_args("diesel_cli", &["--no-default-features", "--features=postgres-bundled"]);

pub static MDBOOK: Cli = Cli::install_crate("mdbook");

pub static TRUNK: Cli = Cli::install_crate_with_args("trunk", &["--locked"]);
