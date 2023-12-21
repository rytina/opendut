use std::fs;
use std::path::PathBuf;

use anyhow::anyhow;

use crate::{Package, Target};
use crate::core::types::parsing::package::PackageSelection;
use crate::core::types::parsing::target::TargetSelection;

const PACKAGE: Package = Package::Edgar;


/// Tasks available or specific for EDGAR
#[derive(Debug, clap::Parser)]
#[command(alias="opendut-edgar")]
pub struct EdgarCli {
    #[command(subcommand)]
    pub task: TaskCli,
}

#[derive(Debug, clap::Subcommand)]
pub enum TaskCli {
    Build(crate::tasks::build::BuildCli),
    Distribution(crate::tasks::distribution::DistributionCli),
    Licenses(crate::tasks::licenses::LicensesCli),

    #[command(hide=true)]
    /// Download the NetBird Client artifact, as it normally happens when building a distribution.
    /// Intended for parallelization in CI/CD.
    DistributionNetbirdClient {
        #[arg(long, default_value_t)]
        target: TargetSelection,
    },
    #[command(hide=true)]
    DistributionCopyLicenseJson(crate::tasks::distribution::copy_license_json::DistributionCopyLicenseJsonCli),
    #[command(hide=true)]
    DistributionBundleFiles(crate::tasks::distribution::bundle::DistributionBundleFilesCli),
    #[command(hide=true)]
    DistributionValidateContents(crate::tasks::distribution::validate::DistributionValidateContentsCli),
}

impl EdgarCli {
    pub fn default_handling(self) -> anyhow::Result<()> {
        match self.task {
            TaskCli::Build(crate::tasks::build::BuildCli { target }) => {
                for target in target.iter() {
                    build::build_release(target)?;
                }
            }
            TaskCli::Distribution(crate::tasks::distribution::DistributionCli { target }) => {
                for target in target.iter() {
                    distribution::edgar_distribution(target)?;
                }
            }
            TaskCli::Licenses(implementation) => {
                implementation.default_handling(PackageSelection::Single(PACKAGE))?;
            }

            TaskCli::DistributionNetbirdClient { target } => {
                for target in target.iter() {
                    distribution::netbird::netbird_client_distribution(target)?;
                }
            }
            TaskCli::DistributionCopyLicenseJson(implementation) => {
                implementation.default_handling(PACKAGE)?;
            }
            TaskCli::DistributionBundleFiles(implementation) => {
                implementation.default_handling(PACKAGE)?;
            }
            TaskCli::DistributionValidateContents(crate::tasks::distribution::validate::DistributionValidateContentsCli { target }) => {
                for target in target.iter() {
                    distribution::validate::validate_contents(target)?;
                }
            }
        };
        Ok(())
    }
}


pub mod build {
    use super::*;

    pub fn build_release(target: Target) -> anyhow::Result<()> {
        crate::tasks::build::build_release(PACKAGE, target)
    }
    pub fn out_dir(target: Target) -> PathBuf {
        crate::tasks::build::out_dir(PACKAGE, target)
    }
}

pub mod distribution {
    use crate::tasks::distribution::copy_license_json::SkipGenerate;

    use super::*;

    #[tracing::instrument]
    pub fn edgar_distribution(target: Target) -> anyhow::Result<()> {
        use crate::tasks::distribution;

        distribution::clean(PACKAGE, target)?;

        crate::tasks::build::build_release(PACKAGE, target)?;

        distribution::collect_executables(PACKAGE, target)?;

        netbird::netbird_client_distribution(target)?;
        distribution::copy_license_json::copy_license_json(PACKAGE, target, SkipGenerate::No)?;

        distribution::bundle::bundle_files(PACKAGE, target)?;

        validate::validate_contents(target)?;

        Ok(())
    }


    pub mod netbird {
        use super::*;

        #[tracing::instrument]
        pub fn netbird_client_distribution(target: Target) -> anyhow::Result<()> {
            //Modelled after documentation here: https://docs.netbird.io/how-to/getting-started#binary-install

            let metadata = crate::metadata::cargo();
            let version = metadata.workspace_metadata["ci"]["netbird"]["version"].as_str()
                .ok_or(anyhow!("NetBird version not defined."))?;

            let os = "linux";

            let arch = match target {
                Target::X86_64 => "amd64",
                Target::Arm64 => "arm64",
                Target::Armhf => "armv6",
            };

            let folder_name = format!("v{version}");
            let file_name = format!("netbird_{version}_{os}_{arch}.tar.gz");

            let netbird_artifact = download_dir().join(&folder_name).join(&file_name);
            fs::create_dir_all(netbird_artifact.parent().unwrap())?;

            if !netbird_artifact.exists() { //download
                let url = format!("https://github.com/reimarstier/netbird/releases/download/{folder_name}/{file_name}");

                println!("Downloading netbird_{version}_{os}_{arch}.tar.gz...");
                let bytes = reqwest::blocking::get(url)?
                    .error_for_status()?
                    .bytes()?;
                println!("Retrieved {} bytes.", bytes.len());

                fs::write(&netbird_artifact, bytes)
                    .map_err(|cause| anyhow!("Error while writing to '{}': {cause}", netbird_artifact.display()))?;
            }
            assert!(netbird_artifact.exists());

            let out_file = out_file(PACKAGE, target);
            fs::create_dir_all(out_file.parent().unwrap())?;

            fs::copy(&netbird_artifact, &out_file)
                .map_err(|cause| anyhow!("Error while copying from '{}' to '{}': {cause}", netbird_artifact.display(), out_file.display()))?;

            Ok(())
        }

        fn download_dir() -> PathBuf {
            crate::constants::target_dir().join("netbird")
        }

        pub fn out_file(package: Package, target: Target) -> PathBuf {
            crate::tasks::distribution::out_package_dir(package, target).join("install").join("netbird.tar.gz")
        }
    }

    pub mod validate {
        use std::fs::File;

        use assert_fs::prelude::*;
        use flate2::read::GzDecoder;
        use predicates::path;

        use crate::core::util::file::ChildPathExt;
        use crate::tasks::distribution::bundle;

        use super::*;

        #[tracing::instrument]
        pub fn validate_contents(target: Target) -> anyhow::Result<()> {

            let unpack_dir = {
                let unpack_dir = assert_fs::TempDir::new()?;
                let archive = bundle::out_file(PACKAGE, target);
                let mut archive = tar::Archive::new(GzDecoder::new(File::open(archive)?));
                archive.set_preserve_permissions(true);
                archive.unpack(&unpack_dir)?;
                unpack_dir
            };

            let edgar_dir = unpack_dir.child("opendut-edgar");
            edgar_dir.assert(path::is_dir());

            let opendut_edgar_executable = edgar_dir.child("opendut-edgar");
            let install_dir = edgar_dir.child("install");
            let licenses_dir = edgar_dir.child("licenses");

            edgar_dir.dir_contains_exactly_in_order(vec![
                &install_dir,
                &licenses_dir,
                &opendut_edgar_executable,
            ]);

            opendut_edgar_executable.assert_non_empty_file();
            install_dir.assert(path::is_dir());
            licenses_dir.assert(path::is_dir());

            {   //validate install dir contents
                let netbird_archive = install_dir.child("netbird.tar.gz");

                install_dir.dir_contains_exactly_in_order(vec![
                    &netbird_archive,
                ]);

                netbird_archive.assert_non_empty_file();
            }

            {   //validate licenses dir contents
                let licenses_edgar_file = licenses_dir.child("opendut-edgar.licenses.json");

                licenses_dir.dir_contains_exactly_in_order(vec![
                    &licenses_edgar_file,
                ]);

                licenses_edgar_file.assert_non_empty_file();
            }

            Ok(())
        }
    }
}
