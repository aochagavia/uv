use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glob::{glob, GlobError, PatternError};
use tracing::debug;

use uv_fs::Simplified;
use uv_normalize::PackageName;

use crate::pyproject::{PyProjectToml, Source, ToolUvWorkspace};

#[derive(thiserror::Error, Debug)]
pub(crate) enum DiscoverError {
    #[error("No `pyproject.toml` found in current directory or any parent directory")]
    MissingPyprojectToml,

    #[error("Failed to find directories for glob: `{0}`")]
    Pattern(String, #[source] PatternError),

    #[error("Invalid glob: `{0}`")]
    Glob(String, #[source] GlobError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error("No `project` section found in: {}", _0.simplified_display())]
    MissingProject(PathBuf),
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(crate) struct WorkspaceMember {
    /// The path to the project root.
    pub(crate) root: PathBuf,
    pub(crate) pyproject_toml: PyProjectToml,
    // TODO(konsti): Add the metadata we want to use later here.
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct ProjectWorkspace {
    /// The path to the project root.
    project_root: PathBuf,
    /// The name of the package.
    project_name: PackageName,
    /// The path to the workspace root.
    workspace_root: PathBuf,
    /// The members of the workspace.
    workspace_packages: BTreeMap<PackageName, WorkspaceMember>,
    /// The source table for the workspace declaration.
    workspace_sources: BTreeMap<PackageName, Source>,
}

impl ProjectWorkspace {
    pub(crate) fn project_pyproject_toml(&self) -> PathBuf {
        self.project_root.join("pyproject.toml")
    }

    pub(crate) fn workspace_sources(&self) -> &BTreeMap<PackageName, Source> {
        &self.workspace_sources
    }

    pub(crate) fn workspace_packages(&self) -> &BTreeMap<PackageName, WorkspaceMember> {
        &self.workspace_packages
    }

    /// Read a pyproject.toml and resolve the workspace, or return `None` if the pyproject.toml
    /// doesn't match the schema.
    pub(crate) fn from_pyproject_toml(
        pyproject_path: &PathBuf,
    ) -> Result<Option<Self>, DiscoverError> {
        let contents = fs_err::read_to_string(&pyproject_path)?;
        let Ok(pyproject_toml) = toml::from_str::<PyProjectToml>(&contents) else {
            // Doesn't match the schema, it might e.g. be using hatch's relative path syntax.
            // TODO(konstin): Exit on dynamic that we can't handle?
            return Ok(None);
        };

        // Extract the package name.
        let Some(project) = pyproject_toml.project.clone() else {
            return Err(DiscoverError::MissingProject(pyproject_path.to_path_buf()));
        };

        let project_workspace = Self::from_project(
            pyproject_path
                .parent()
                .expect("pyproject.toml must have a parent")
                .to_path_buf(),
            pyproject_toml,
            project.name,
        )?;
        Ok(Some(project_workspace))
    }

    /// Find the current project.
    pub(crate) fn discover(path: impl AsRef<Path>) -> Result<Self, DiscoverError> {
        debug!("Project root: `{}`", path.as_ref().simplified_display());

        let Some((project_path, project, project_name)) = Self::read_project(path.as_ref())? else {
            // We require that you are in a project.
            return Err(DiscoverError::MissingPyprojectToml);
        };

        Self::from_project(project_path, project, project_name)
    }

    fn from_project(
        project_path: PathBuf,
        project: PyProjectToml,
        project_name: PackageName,
    ) -> Result<Self, DiscoverError> {
        let mut workspace = project
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.workspace.as_ref())
            .map(|workspace| (project_path.clone(), workspace.clone(), project.clone()));

        if workspace.is_none() {
            workspace = Self::find_workspace(&project_path)?;
        }

        let mut workspace_members = BTreeMap::new();
        workspace_members.insert(
            project_name.clone(),
            WorkspaceMember {
                root: project_path.clone(),
                pyproject_toml: project.clone(),
            },
        );

        match workspace {
            Some((workspace_root, workspace_definition, project_in_workspace_root)) => {
                debug!("Workspace root: `{}`", workspace_root.simplified_display());
                if workspace_root != project_path {
                    // TODO(konsti): serde error context.
                    let pyproject_toml = toml::from_str(&fs_err::read_to_string(
                        workspace_root.join("pyproject.toml"),
                    )?)?;

                    if let Some(project) = &project_in_workspace_root.project {
                        workspace_members.insert(
                            project.name.clone(),
                            WorkspaceMember {
                                root: workspace_root.clone(),
                                pyproject_toml,
                            },
                        );
                    };
                }
                for member_glob in workspace_definition.members.unwrap_or_default() {
                    let absolute_glob = workspace_root
                        .join(member_glob.as_str())
                        .to_string_lossy()
                        .to_string();
                    for member_root in glob(&absolute_glob)
                        .map_err(|err| DiscoverError::Pattern(absolute_glob.to_string(), err))?
                    {
                        // TODO(konsti): Filter already seen.
                        // TODO(konsti): Error context? There's no fs_err here.
                        let member_root = member_root
                            .map_err(|err| DiscoverError::Glob(absolute_glob.to_string(), err))?;
                        // Read the `pyproject.toml`.
                        let contents = fs_err::read_to_string(&member_root.join("pyproject.toml"))?;
                        let pyproject_toml: PyProjectToml = toml::from_str(&contents)?;

                        // Extract the package name.
                        let Some(project) = pyproject_toml.project.clone() else {
                            return Err(DiscoverError::MissingProject(member_root));
                        };

                        // TODO(konsti): serde error context.
                        let pyproject_toml = toml::from_str(&fs_err::read_to_string(
                            workspace_root.join("pyproject.toml"),
                        )?)?;
                        let member = WorkspaceMember {
                            root: member_root.clone(),
                            pyproject_toml,
                        };
                        workspace_members.insert(project.name, member);
                    }
                }
                let workspace_sources = project_in_workspace_root
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.sources.clone())
                    .unwrap_or_default();

                // TODO(konsti): check_above();
                return Ok(Self {
                    project_root: project_path,
                    project_name,
                    workspace_root,
                    workspace_packages: workspace_members,
                    workspace_sources,
                });
            }
            None => {
                // The project and the workspace root are identical
                debug!("No explicit workspace root found");
                // TODO(konsti): check_above();
                return Ok(Self {
                    project_root: project_path.clone(),
                    project_name,
                    workspace_root: project_path,
                    workspace_packages: workspace_members,
                    workspace_sources: BTreeMap::default(),
                });
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn dummy(root: &Path, project_name: &PackageName) -> Self {
        Self {
            project_root: root.to_path_buf(),
            project_name: project_name.clone(),
            workspace_root: root.to_path_buf(),
            workspace_packages: Default::default(),
            workspace_sources: Default::default(),
        }
    }

    fn read_project(
        path: &Path,
    ) -> Result<Option<(PathBuf, PyProjectToml, PackageName)>, DiscoverError> {
        let pyproject_path = path.join("pyproject.toml");

        // Read the `pyproject.toml`.
        let contents = fs_err::read_to_string(&pyproject_path)?;
        let pyproject_toml: PyProjectToml = toml::from_str(&contents)?;

        // Extract the package name.
        let Some(project) = pyproject_toml.project.clone() else {
            return Err(DiscoverError::MissingProject(pyproject_path));
        };

        return Ok(Some((path.to_path_buf(), pyproject_toml, project.name)));
    }

    /// Find the workspace root above the current project, if any.
    fn find_workspace(
        path: &Path,
    ) -> Result<Option<(PathBuf, ToolUvWorkspace, PyProjectToml)>, DiscoverError> {
        for ancestor in path.ancestors() {
            let pyproject_path = ancestor.join("pyproject.toml");
            if !pyproject_path.exists() {
                continue;
            }
            debug!(
                "Found project root: {}",
                pyproject_path.simplified_display()
            );

            // Read the `pyproject.toml`.
            let contents = fs_err::read_to_string(&pyproject_path)?;
            let pyproject_toml: PyProjectToml = toml::from_str(&contents)?;

            return if let Some(workspace) = pyproject_toml
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.workspace.as_ref())
            {
                // Check if we're in the excludes of a workspace.
                for exclude_glob in workspace.exclude.iter().flatten() {
                    let absolute_glob = ancestor
                        .join(exclude_glob.as_str())
                        .to_string_lossy()
                        .to_string();
                    for excluded_root in glob(&absolute_glob)
                        .map_err(|err| DiscoverError::Pattern(absolute_glob.to_string(), err))?
                    {
                        let excluded_root = excluded_root
                            .map_err(|err| DiscoverError::Glob(absolute_glob.to_string(), err))?;
                        if excluded_root == path {
                            debug!(
                                "Found workspace root `{}`, but project is excluded.",
                                ancestor.simplified_display()
                            );
                            return Ok(None);
                        }
                    }
                }

                debug!("Found workspace root: `{}`", ancestor.simplified_display());

                // We found a workspace root.
                Ok(Some((
                    ancestor.to_path_buf(),
                    workspace.clone(),
                    pyproject_toml,
                )))
            } else if let Some(_) = pyproject_toml.project.clone() {
                // We're in a directory of another project, e.g. tests or examples.
                // Example:
                // ```
                // albatross
                // ├── examples
                // │   └── bird-feeder [CURRENT DIRECTORY]
                // │       ├── pyproject.toml
                // │       └── src
                // │           └── bird_feeder
                // │               └── __init__.py
                // ├── pyproject.toml
                // └── src
                //     └── albatross
                //         └── __init__.py
                // ```
                // The current project is the example (non-workspace) `bird-feeder` in `albatross`,
                // we ignore all `albatross` is doing and any potential workspace it might be
                // contained in.
                debug!(
                    "Project is contained in non-workspace project: `{}`",
                    ancestor.simplified_display()
                );
                Ok(None)
            } else {
                // We require that a `project.toml` file either declares a workspace or a project.
                Err(DiscoverError::MissingProject(pyproject_path))
            };
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use insta::assert_json_snapshot;

    use crate::discovery::ProjectWorkspace;

    fn workspace_test(folder: &str) -> (ProjectWorkspace, String) {
        let root_dir = env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts")
            .join("workspaces");
        let project = ProjectWorkspace::discover(root_dir.join(folder)).unwrap();
        let root_escaped = regex::escape(root_dir.to_string_lossy().as_ref());
        (project, root_escaped)
    }

    #[test]
    fn albatross_in_example() {
        let (project, root_escaped) = workspace_test("albatross-root-workspace");
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(project, @r###"
            {
              "project": "[ROOT]/albatross-root-workspace",
              "name": "abatross",
              "workspace": "[ROOT]/albatross-root-workspace",
              "workspace_members": {
                "abatross": {
                  "root": "[ROOT]/albatross-root-workspace"
                },
                "bird-feeder": {
                  "root": "[ROOT]/albatross-root-workspace/packages/bird-feeder"
                }
              }
            }
            "###);
        });
    }

    #[test]
    fn albatross_project_in_excluded() {
        let (project, root_escaped) = workspace_test("albatross-project-in-excluded");
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(project, @r###"
            {
              "project": "[ROOT]/albatross-project-in-excluded",
              "name": "abatross",
              "workspace": "[ROOT]/albatross-project-in-excluded",
              "workspace_members": {
                "abatross": {
                  "root": "[ROOT]/albatross-project-in-excluded"
                }
              }
            }
            "###);
        });
    }

    #[test]
    fn albatross_root_workspace() {
        let (project, root_escaped) = workspace_test("albatross-root-workspace");
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(project, @r###"
            {
              "project": "[ROOT]/albatross-root-workspace",
              "name": "abatross",
              "workspace": "[ROOT]/albatross-root-workspace",
              "workspace_members": {
                "abatross": {
                  "root": "[ROOT]/albatross-root-workspace"
                },
                "bird-feeder": {
                  "root": "[ROOT]/albatross-root-workspace/packages/bird-feeder"
                }
              }
            }
            "###);
        });
    }

    #[test]
    fn albatross_virtual_workspace() {
        let (project, root_escaped) = workspace_test("albatross-virtual-workspace");
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(project, @r###"
            {
              "project": "[ROOT]/albatross-virtual-workspace",
              "name": "abatross",
              "workspace": "[ROOT]/albatross-virtual-workspace",
              "workspace_members": {
                "abatross": {
                  "root": "[ROOT]/albatross-virtual-workspace"
                },
                "bird-feeder": {
                  "root": "[ROOT]/albatross-virtual-workspace/packages/bird-feeder"
                }
              }
            }
            "###);
        });
    }

    #[test]
    fn albatross_just_project() {
        let (project, root_escaped) = workspace_test("albatross-just-project");
        let filters = vec![(root_escaped.as_str(), "[ROOT]")];
        insta::with_settings!({filters => filters}, {
            assert_json_snapshot!(project, @r###"
            {
              "project": "[ROOT]/albatross-just-project",
              "name": "abatross",
              "workspace": "[ROOT]/albatross-just-project",
              "workspace_members": {
                "abatross": {
                  "root": "[ROOT]/albatross-just-project"
                }
              }
            }
            "###);
        });
    }
}
