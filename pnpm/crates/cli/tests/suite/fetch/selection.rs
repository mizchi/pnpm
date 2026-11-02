use crate::_utils::{ManifestDeps, WorkspaceFixture, assert_success};
use pnpm_lockfile::{PackageKey, SymlinkedLockfile};
use std::fs;

const SELECTED_DEP: &str = "@pnpm.e2e/pkg-with-1-dep";
const UNSELECTED_DEP: &str = "@pnpm.e2e/qar";
const LIB_DEP: &str = "@pnpm.e2e/foo";
const DEV_DEP: &str = "@pnpm.e2e/bar";

#[test]
fn fetch_directory_filter_uses_only_the_selected_lockfile_importer_without_manifests() {
    let fixture = WorkspaceFixture::new();
    let selected = fixture.project(
        "selected",
        "selected",
        ManifestDeps { prod: &[(SELECTED_DEP, "100.0.0")], ..Default::default() },
    );
    let unselected = fixture.project(
        "unselected",
        "unselected",
        ManifestDeps { prod: &[(UNSELECTED_DEP, "100.0.0")], ..Default::default() },
    );
    fixture.run(["install", "--lockfile-only"]);
    let wanted = fs::read(fixture.workspace.join("pnpm-lock.yaml")).unwrap();
    fs::remove_dir_all(&selected).unwrap();
    fs::remove_dir_all(&unselected).unwrap();

    fixture.run(["fetch", "--filter", "./packages/selected", "--fail-if-no-match"]);

    assert!(fixture.slot(SELECTED_DEP, "100.0.0").exists(), "selected dependency must be fetched");
    assert!(
        !fixture.slot(UNSELECTED_DEP, "100.0.0").exists(),
        "unselected dependency must not be fetched",
    );
    assert!(
        fixture.slot("@pnpm.e2e/dep-of-pkg-with-1-dep", "100.1.0").exists(),
        "transitive dependency must be fetched",
    );
    assert_eq!(fs::read(fixture.workspace.join("pnpm-lock.yaml")).unwrap(), wanted);
    assert!(!selected.exists(), "fetch must not create the missing project");
}

#[test]
fn fetch_name_filter_reads_names_without_creating_project_links() {
    let fixture = WorkspaceFixture::new();
    let selected = fixture.project(
        "selected",
        "selected",
        ManifestDeps { prod: &[(SELECTED_DEP, "100.0.0")], ..Default::default() },
    );
    fixture.project(
        "unselected",
        "unselected",
        ManifestDeps { prod: &[(UNSELECTED_DEP, "100.0.0")], ..Default::default() },
    );
    fixture.run(["install", "--lockfile-only"]);

    fixture.run(["fetch", "--filter", "selected"]);

    assert!(fixture.slot(SELECTED_DEP, "100.0.0").exists());
    assert!(!fixture.slot(UNSELECTED_DEP, "100.0.0").exists());
    assert!(!selected.join("node_modules").exists(), "fetch must not create project links");
}

fn workspace_with_links() -> WorkspaceFixture {
    let fixture = WorkspaceFixture::new();
    fixture.project(
        "app",
        "app",
        ManifestDeps {
            prod: &[(SELECTED_DEP, "100.0.0"), ("lib", "workspace:*")],
            dev: &[("dev-helper", "workspace:*")],
            ..Default::default()
        },
    );
    fixture.project(
        "lib",
        "lib",
        ManifestDeps {
            prod: &[(LIB_DEP, "100.0.0")],
            dev: &[(DEV_DEP, "100.0.0")],
            ..Default::default()
        },
    );
    fixture.project(
        "dev-helper",
        "dev-helper",
        ManifestDeps { prod: &[(UNSELECTED_DEP, "100.0.0")], ..Default::default() },
    );
    fixture.run(["install", "--lockfile-only"]);
    fixture
}

#[test]
fn fetch_follows_locked_workspace_dependencies_without_their_manifests() {
    let fixture = workspace_with_links();
    fs::remove_dir_all(fixture.workspace.join("packages")).unwrap();

    fixture.run(["fetch", "--filter", "{packages/app}..."]);

    for name in [SELECTED_DEP, LIB_DEP, DEV_DEP, UNSELECTED_DEP] {
        assert!(fixture.slot(name, "100.0.0").exists(), "missing dependency {name}");
    }
    assert!(!fixture.workspace.join("packages").exists());
}

#[test]
fn fetch_prod_filter_excludes_development_workspace_edges_and_packages() {
    let fixture = workspace_with_links();
    fs::remove_dir_all(fixture.workspace.join("packages")).unwrap();

    fixture.run(["fetch", "--filter-prod", "{packages/app}...", "--prod"]);

    assert!(fixture.slot(SELECTED_DEP, "100.0.0").exists());
    assert!(fixture.slot(LIB_DEP, "100.0.0").exists());
    assert!(!fixture.slot(DEV_DEP, "100.0.0").exists());
    assert!(!fixture.slot(UNSELECTED_DEP, "100.0.0").exists());
}

#[test]
fn fetch_directory_filter_is_relative_to_the_invocation_directory() {
    let fixture = workspace_with_links();

    fixture.run_at(&fixture.workspace.join("packages/lib"), ["fetch", "--filter", "{.}"]);

    assert!(fixture.slot(LIB_DEP, "100.0.0").exists());
    assert!(fixture.slot(DEV_DEP, "100.0.0").exists());
    assert!(!fixture.slot(SELECTED_DEP, "100.0.0").exists());
}

#[test]
fn fetch_unmatched_filter_skips_fetching_and_honors_fail_if_no_match() {
    let fixture = workspace_with_links();
    let output = fixture.command_at(&fixture.workspace, ["fetch", "--filter", "./missing"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("No projects matched"));
    assert!(!fixture.slot(SELECTED_DEP, "100.0.0").exists());

    let output = fixture.command_at(
        &fixture.workspace,
        ["fetch", "--filter", "./missing", "--fail-if-no-match"],
    );
    assert!(!output.status.success(), "an unmatched filter must fail when requested: {output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("No projects matched"));
}

#[test]
fn fetch_workspace_root_adds_the_root_to_an_explicit_selection() {
    let fixture = workspace_with_links();
    fixture.write_root_manifest(
        "root",
        ManifestDeps { dev: &[(DEV_DEP, "100.0.0")], ..Default::default() },
    );
    fixture.run(["install", "--lockfile-only"]);

    fixture.run(["fetch", "--filter", "./packages/app", "--workspace-root"]);

    assert!(fixture.slot(SELECTED_DEP, "100.0.0").exists());
    assert!(fixture.slot(DEV_DEP, "100.0.0").exists());
    assert!(!fixture.slot(LIB_DEP, "100.0.0").exists());
}

#[test]
fn filtered_fetch_preserves_an_installed_unselected_project() {
    let fixture = workspace_with_links();
    fixture.run(["install"]);
    let unselected_package = fixture.workspace
        .join("packages/dev-helper/node_modules")
        .join(UNSELECTED_DEP)
        .join("package.json");
    let original = fs::read(&unselected_package).unwrap();
    let state_path = fixture.workspace.join("node_modules/.pnpm-workspace-state-v1.json");
    let state = fs::read(&state_path).unwrap();

    fixture.run(["fetch", "--filter", "{packages/lib}"]);

    assert_eq!(fs::read(&unselected_package).unwrap(), original);
    assert_eq!(fs::read(&state_path).unwrap(), state);
}

#[test]
fn filtered_fetch_rejects_a_missing_required_snapshot() {
    let fixture = workspace_with_links();
    let mut lockfile = fixture.wanted();
    let key: PackageKey = format!("{SELECTED_DEP}@100.0.0").parse().unwrap();
    assert!(
        lockfile.snapshots
            .as_mut()
            .unwrap()
            .remove(&key)
            .is_some()
    );
    lockfile
        .save_to_path(&fixture.workspace.join("pnpm-lock.yaml"), SymlinkedLockfile::Refuse)
        .unwrap();

    let output = fixture.command_at(&fixture.workspace, ["fetch", "--filter", "{packages/app}"]);

    assert!(!output.status.success(), "a broken selected graph must fail: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ERR_PNPM_LOCKFILE_MISSING_DEPENDENCY"), "stderr: {stderr}");
}
