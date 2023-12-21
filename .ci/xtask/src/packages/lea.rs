use std::path::PathBuf;
use std::process::Command;
use crate::Arch;
use crate::core::dependency::Crate;

use crate::core::types::parsing::package::PackageSelection;
use crate::Package;
use crate::util::RunRequiringSuccess;

const PACKAGE: Package = Package::Lea;

/// Tasks available or specific for LEA
#[derive(Debug, clap::Parser)]
#[command(alias="opendut-lea")]
pub struct LeaCli {
    #[command(subcommand)]
    pub task: TaskCli,
}

#[derive(Debug, clap::Subcommand)]
pub enum TaskCli {
    /// Perform a release build, without bundling a distribution.
    Build,
    /// Start a development server for LEA which watches for file changes.
    Watch,
    Licenses(crate::tasks::licenses::LicensesCli),
}

impl LeaCli {
    pub fn default_handling(self) -> crate::Result {
        match self.task {
            TaskCli::Build => build::build_release()?,
            TaskCli::Watch => watch::watch()?,
            TaskCli::Licenses(implementation) => {
                implementation.default_handling(PackageSelection::Single(PACKAGE))?;
            }
        };
        Ok(())
    }
}

pub mod build {
    use super::*;

    #[tracing::instrument]
    pub fn build_release() -> crate::Result {
        install_requirements()?;

        let working_dir = self_dir();
        let out_dir = out_dir();

        Command::new("trunk")
            .args([
                "build",
                "--release",
                "--dist", &out_dir.display().to_string(),
            ])
            .current_dir(working_dir)
            .run_requiring_success();

        Ok(())
    }

    pub fn out_dir() -> PathBuf {
        self_dir().join("dist")
    }
}

pub mod watch {
    use super::*;

    #[tracing::instrument]
    pub fn watch() -> crate::Result {
        install_requirements()?;

        Command::new("trunk")
            .arg("watch")
            .current_dir(self_dir())
            .run_requiring_success();
        Ok(())
    }
}

#[tracing::instrument]
fn install_requirements() -> crate::Result {
    crate::util::install_toolchain(Arch::Wasm)?;

    crate::util::install_crate(Crate::Trunk)?;

    Ok(())
}

pub fn self_dir() -> PathBuf {
    crate::constants::workspace_dir()
        .join(PACKAGE.ident())
}
