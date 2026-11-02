use super::{
    AutoExcludeRoot, filter_against, recursive_filter_options, unmatched::unmatched_filters,
};
use pnpm_config::Config;
use pnpm_lockfile::{Lockfile, ProjectSnapshot};
use pnpm_package_manifest::DependencyGroup;
use pnpm_workspace_projects_filter::{
    BaseProject, ProjectGraph, ProjectGraphNode, parse_project_selector,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone)]
struct LockedProject<'a> {
    root_dir: PathBuf,
    name: Option<String>,
    snapshot: &'a ProjectSnapshot,
}

impl BaseProject for LockedProject<'_> {
    fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    fn manifest_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn merged_dependencies(&self, ignore_dev_deps: bool) -> Vec<(String, String)> {
        self.snapshot
            .dependencies_by_groups(
                [DependencyGroup::Prod, DependencyGroup::Optional]
                    .into_iter()
                    .chain((!ignore_dev_deps).then_some(DependencyGroup::Dev)),
            )
            .map(|(name, spec)| (name.to_string(), spec.specifier.clone()))
            .collect()
    }
}

/// Select lockfile importers without requiring their manifests for path selectors.
/// Name selectors read the available manifests; workspace edges come from the lockfile.
pub fn selected_lockfile_importer_ids(
    config: &Config,
    (project_dir, lockfile_dir): (&Path, &Path),
    lockfile: &Lockfile,
) -> miette::Result<HashSet<String>> {
    let projects = locked_projects(config, project_dir, lockfile_dir, lockfile)?;
    let selected = selected_project_dirs(config, project_dir, &projects)?;
    Ok(selected
        .iter()
        .map(|dir| pnpm_workspace::importer_id_from_root_dir(lockfile_dir, dir))
        .collect())
}

fn selected_project_dirs(
    config: &Config,
    project_dir: &Path,
    projects: &[LockedProject<'_>],
) -> miette::Result<Vec<PathBuf>> {
    let graph = projects_graph(
        projects,
        &[DependencyGroup::Prod, DependencyGroup::Optional, DependencyGroup::Dev],
    );
    let root_selector = AutoExcludeRoot::Disabled.root_selector(config, project_dir);
    let walk_options = recursive_filter_options(config, project_dir);
    let mut selected = filter_against(
        &graph,
        &config.filter,
        root_selector.as_deref().filter(|_| config.filter_prod.is_empty()),
        false,
        project_dir,
        &walk_options,
    )?;
    if !config.filter_prod.is_empty() {
        selected.extend(filter_against(
            &projects_graph(projects, &[DependencyGroup::Prod, DependencyGroup::Optional]),
            &config.filter_prod,
            root_selector.as_deref(),
            true,
            project_dir,
            &walk_options,
        )?);
    }
    if let Some(unmatched) = unmatched_filters(selected.len(), graph.len(), config, project_dir) {
        return Err(unmatched.report());
    }
    Ok(selected)
}

fn locked_projects<'a>(
    config: &Config,
    project_dir: &Path,
    lockfile_dir: &Path,
    lockfile: &'a Lockfile,
) -> miette::Result<Vec<LockedProject<'a>>> {
    let needs_names = config.filter
        .iter()
        .chain(&config.filter_prod)
        .any(|filter| parse_project_selector(filter, project_dir).name_pattern.is_some());
    lockfile.importers
        .iter()
        .map(|(id, snapshot)| {
            pnpm_package_manager::validate_importer_id(id)?;
            let root_dir = pnpm_fs::lexical_normalize(&pnpm_package_manager::importer_root_dir(
                lockfile_dir,
                id,
            ));
            let name = if needs_names {
                pnpm_workspace::safe_read_project_manifest_only(&root_dir)?
                    .and_then(|manifest| {
                        manifest
                            .value()
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
            } else {
                None
            };
            Ok(LockedProject { root_dir, name, snapshot })
        })
        .collect()
}

fn projects_graph<'a>(
    projects: &[LockedProject<'a>],
    groups: &[DependencyGroup],
) -> ProjectGraph<LockedProject<'a>> {
    let known_dirs: HashSet<_> = projects
        .iter()
        .map(|project| &project.root_dir)
        .collect();
    projects
        .iter()
        .map(|project| {
            let dependencies = project.snapshot
                .dependencies_by_groups(groups.iter().copied())
                .filter_map(|(_, spec)| spec.version.as_link_target())
                .map(|target| pnpm_fs::lexical_normalize(&project.root_dir.join(target)))
                .filter(|target| known_dirs.contains(target))
                .collect();
            (project.root_dir.clone(), ProjectGraphNode { package: project.clone(), dependencies })
        })
        .collect()
}
