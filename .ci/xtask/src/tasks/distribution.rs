use std::fs;
use std::path::PathBuf;

use crate::{constants, Package, Target};
use crate::types::parsing::target::TargetSelection;


/// Build and bundle a release distribution
#[derive(Debug, clap::Parser)]
#[command(alias="dist")]
pub struct DistributionCli {
    #[arg(long, default_value_t)]
    pub target: TargetSelection,
}

#[tracing::instrument]
pub fn clean(package: &Package, target: &Target) -> anyhow::Result<()> {
    let package_dir = out_package_dir(package, target);
    if package_dir.exists() {
        fs::remove_dir_all(&package_dir)?;
        log::debug!("Cleaned distribution directory at: {package_dir:?}");
    }
    Ok(())
}

#[tracing::instrument]
pub fn collect_executables(package: &Package, target: &Target) -> anyhow::Result<()> {

    let out_dir = out_package_dir(package, target);
    fs::create_dir_all(&out_dir)?;

    fs::copy(
        crate::tasks::build::out_dir(package, target),
        out_dir.join(package.ident()),
    )?;
    Ok(())
}


pub mod bundle {
    use std::fs;
    use crate::core::types::{Package, Target};
    use crate::core::types::parsing::target::TargetSelection;
    use crate::tasks::distribution::{out_arch_dir, out_package_dir};

    /// Directly bundle files from the distribution directory, as it normally happens when building a distribution.
    /// Intended for parallelization in CI/CD.
    #[derive(Debug, clap::Parser)]
    pub struct DistributionBundleFilesCli {
        #[arg(long, default_value_t)]
        target: TargetSelection,
    }
    impl DistributionBundleFilesCli {
        pub fn handle(&self, package: &Package) -> anyhow::Result<()> {
            for target in self.target.iter() {
                bundle_files(package, &target)?;
            }
            Ok(())
        }
    }

    #[tracing::instrument]
    pub fn bundle_files(package: &Package, target: &Target) -> anyhow::Result<()> {
        use flate2::Compression;
        use flate2::write::GzEncoder;

        let in_dir = out_package_dir(package, target);
        let out_dir = out_arch_dir(target);
        fs::create_dir_all(&out_dir)?;

        let target_triple = target.triple();
        let version = crate::build::PKG_VERSION;

        let file = fs::File::create(
            out_dir.join(format!("{}-{target_triple}-{version}.tar.gz", package.ident()))
        )?;

        let mut tar_gz = tar::Builder::new(
            GzEncoder::new(file, Compression::best())
        );
        tar_gz.append_dir_all(package.ident(), &in_dir)?;
        tar_gz.finish()?;

        fs::remove_dir_all(in_dir)?;

        Ok(())
    }
}


pub mod copy_license_json {
    use super::*;

    /// Copy license files to the distribution directory, as it normally happens when building a distribution.
    /// Intended for parallelization in CI/CD.
    #[derive(Debug, clap::Parser)]
    pub struct CopyLicenseJsonCli {
        #[arg(long, default_value_t)]
        target: TargetSelection,

        #[arg(long)]
        /// Skip the generation of the license files and attempt to copy them directly.
        skip_generate: bool,
    }
    impl CopyLicenseJsonCli {
        pub fn handle(&self, package: &Package) -> anyhow::Result<()> {
            let skip_generate = SkipGenerate::from(self.skip_generate);
            for target in self.target.iter() {
                copy_license_json(package, &target, skip_generate)?;
            }
            Ok(())
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub enum SkipGenerate { Yes, No }
    impl From<bool> for SkipGenerate {
        fn from(value: bool) -> Self {
            if value { SkipGenerate::Yes } else { SkipGenerate::No }
        }
    }

    #[tracing::instrument]
    pub fn copy_license_json(package: &Package, target: &Target, skip_generate: SkipGenerate) -> anyhow::Result<()> {

        match skip_generate {
            SkipGenerate::Yes => log::info!("Skipping generation of licenses, as requested. Directly attempting to copy to target location."),
            SkipGenerate::No => crate::tasks::licenses::json::export_json(package)?,
        };
        let licenses_file = crate::tasks::licenses::json::out_file(package);

        let out_file = out_file(package, target);
        fs::create_dir_all(out_file.parent().unwrap())?;

        fs::copy(licenses_file, out_file)?;

        Ok(())
    }
    pub fn out_file(package: &Package, target: &Target) -> PathBuf {
        let licenses_file_name = format!("{}.licenses.json", package.ident());
        out_package_dir(package, target).join("licenses").join(licenses_file_name)
    }
}

pub fn out_dir() -> PathBuf {
    constants::target_dir().join("distribution")
}

pub fn out_arch_dir(target: &Target) -> PathBuf {
    out_dir().join(target.triple())
}

pub fn out_package_dir(package: &Package, target: &Target) -> PathBuf {
    out_arch_dir(target).join(package.ident())
}
