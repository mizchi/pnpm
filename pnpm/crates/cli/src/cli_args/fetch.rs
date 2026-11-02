use crate::{State, cli_args::pre_command::fetch_locked_package_manager};
use clap::Args;
use miette::Context;
use pnpm_lockfile::MaybeLazyLockfile;
use pnpm_modules_yaml::IncludedDependencies;
use pnpm_package_manager::{Install, ProjectMutation};
use pnpm_package_manifest::DependencyGroup;
use pnpm_reporter::Reporter;
use std::{collections::HashSet, path::Path};

#[derive(Debug, Args)]
pub struct FetchArgs {
    #[clap(short = 'P', long, visible_alias = "production")]
    prod: bool,
    #[clap(short = 'D', long)]
    dev: bool,

    /// Disable pnpm hooks defined in `.pnpmfile.cjs`, including the
    /// pnpmfiles of config dependencies.
    #[clap(long = "ignore-pnpmfile")]
    pub(crate) ignore_pnpmfile: bool,
}

impl FetchArgs {
    pub async fn run<Reporter: self::Reporter + 'static>(
        self,
        state: State,
        selected_importers: Option<HashSet<String>>,
    ) -> miette::Result<()> {
        fetch_locked_package_manager::<Reporter>(state.config, state.lockfile_dir()).await?;
        let lockfile_path = state.lockfile_path();
        let included = IncludedDependencies {
            dependencies: self.prod == self.dev || self.prod,
            dev_dependencies: self.prod == self.dev || self.dev,
            optional_dependencies: state.config.optional,
        };
        let install = fetch_install(&state, &lockfile_path, included);
        if let Some(selected) = selected_importers {
            install.run_for_importers::<Reporter>(&selected).await
        } else {
            install.run::<Reporter>().await
        }
        .wrap_err("fetching dependencies")
    }
}

fn fetch_install<'a>(
    state: &'a State,
    lockfile_path: &'a Path,
    included: IncludedDependencies,
) -> Install<'a, Vec<DependencyGroup>> {
    let mut config = (*state.config).clone();
    config.virtual_store_only = true;
    config.enable_modules_dir = true;
    config.apply_virtual_store_only_derivation();
    let groups = [
        (DependencyGroup::Prod, included.dependencies),
        (DependencyGroup::Dev, included.dev_dependencies),
        (DependencyGroup::Optional, included.optional_dependencies),
    ]
    .into_iter()
    .filter_map(|(group, is_included)| is_included.then_some(group))
    .collect();
    let mut install = Install::new(
        std::sync::Arc::clone(&state.tarball_mem_cache),
        &state.resolved_packages,
        (&state.http_client, std::sync::Arc::clone(&state.http_client)),
        config.leak(),
        &state.manifest,
        MaybeLazyLockfile::Lazy(&state.lockfile),
        groups,
    );
    install.lockfile_policy.frozen = true;
    install.lockfile_policy.ignore_manifest_check = true;
    install.execution.mutation = ProjectMutation::NoInstall;
    install.context.lockfile_path = Some(lockfile_path);
    install
}
