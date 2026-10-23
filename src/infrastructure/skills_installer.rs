use std::fs;
use std::io;
use std::path::Path;

use thiserror::Error;

const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Error, PartialEq)]
pub enum SkillsError {
    #[error("pasta de skills não encontrada: {0}")]
    SourceMissing(String),
    #[error("{path}: {reason}")]
    Io { path: String, reason: String },
    #[error("instalar skills só é suportado em sistemas Unix")]
    Unsupported,
}

#[derive(Debug, PartialEq)]
pub enum SkillOutcome {
    Linked(String),
    AlreadyLinked(String),
    Conflict(String),
}

impl SkillsError {
    fn io(path: &Path) -> impl FnOnce(io::Error) -> SkillsError + '_ {
        move |error| SkillsError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        }
    }
}

pub fn install_skills(source: &Path, target: &Path) -> Result<Vec<SkillOutcome>, SkillsError> {
    if !source.is_dir() {
        return Err(SkillsError::SourceMissing(source.display().to_string()));
    }
    fs::create_dir_all(target).map_err(SkillsError::io(target))?;
    skill_names(source)?
        .into_iter()
        .map(|name| link_skill(&source.join(&name), &target.join(&name), name))
        .collect()
}

fn skill_names(source: &Path) -> Result<Vec<String>, SkillsError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(source).map_err(SkillsError::io(source))? {
        let entry = entry.map_err(SkillsError::io(source))?;
        if entry.path().join(SKILL_FILE_NAME).is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

fn link_skill(skill: &Path, destination: &Path, name: String) -> Result<SkillOutcome, SkillsError> {
    match fs::symlink_metadata(destination) {
        Ok(_) if points_to(destination, skill) => Ok(SkillOutcome::AlreadyLinked(name)),
        Ok(_) => Ok(SkillOutcome::Conflict(name)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_link(skill, destination)?;
            Ok(SkillOutcome::Linked(name))
        }
        Err(error) => Err(SkillsError::io(destination)(error)),
    }
}

fn points_to(link: &Path, skill: &Path) -> bool {
    fs::read_link(link).is_ok_and(|current| current == skill)
}

#[cfg(unix)]
fn create_link(skill: &Path, destination: &Path) -> Result<(), SkillsError> {
    std::os::unix::fs::symlink(skill, destination).map_err(SkillsError::io(destination))
}

#[cfg(not(unix))]
fn create_link(_skill: &Path, _destination: &Path) -> Result<(), SkillsError> {
    Err(SkillsError::Unsupported)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn write_skill(source: &Path, name: &str) {
        let directory = source.join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(SKILL_FILE_NAME), "---\nname: x\n---\n").unwrap();
    }

    #[test]
    fn links_every_skill_into_the_target_directory() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_skill(source.path(), "mosaic-plugin-jira");
        write_skill(source.path(), "mosaic-widget-authoring");

        let outcomes = install_skills(source.path(), target.path()).unwrap();

        assert_eq!(
            outcomes,
            vec![
                SkillOutcome::Linked("mosaic-plugin-jira".to_string()),
                SkillOutcome::Linked("mosaic-widget-authoring".to_string()),
            ]
        );
        let link = target.path().join("mosaic-plugin-jira");
        assert_eq!(
            fs::read_link(&link).unwrap(),
            source.path().join("mosaic-plugin-jira")
        );
        assert!(link.join(SKILL_FILE_NAME).is_file());
    }

    #[test]
    fn creates_the_target_directory_when_missing() {
        let source = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        write_skill(source.path(), "mosaic-plugin-jira");
        let target = base.path().join(".claude/skills");

        install_skills(source.path(), &target).unwrap();

        assert!(target.join("mosaic-plugin-jira").exists());
    }

    #[test]
    fn running_twice_reports_already_linked() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_skill(source.path(), "mosaic-plugin-jira");
        install_skills(source.path(), target.path()).unwrap();

        let outcomes = install_skills(source.path(), target.path()).unwrap();

        assert_eq!(
            outcomes,
            vec![SkillOutcome::AlreadyLinked(
                "mosaic-plugin-jira".to_string()
            )]
        );
    }

    #[test]
    fn never_overwrites_an_existing_skill_of_the_same_name() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_skill(source.path(), "mosaic-plugin-jira");
        let own = target.path().join("mosaic-plugin-jira");
        fs::create_dir_all(&own).unwrap();
        fs::write(own.join(SKILL_FILE_NAME), "minha versão").unwrap();

        let outcomes = install_skills(source.path(), target.path()).unwrap();

        assert_eq!(
            outcomes,
            vec![SkillOutcome::Conflict("mosaic-plugin-jira".to_string())]
        );
        assert_eq!(
            fs::read_to_string(own.join(SKILL_FILE_NAME)).unwrap(),
            "minha versão"
        );
    }

    #[test]
    fn a_link_to_somewhere_else_is_a_conflict() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write_skill(source.path(), "mosaic-plugin-jira");
        symlink(elsewhere.path(), target.path().join("mosaic-plugin-jira")).unwrap();

        let outcomes = install_skills(source.path(), target.path()).unwrap();

        assert_eq!(
            outcomes,
            vec![SkillOutcome::Conflict("mosaic-plugin-jira".to_string())]
        );
    }

    #[test]
    fn ignores_entries_that_are_not_skills() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_skill(source.path(), "mosaic-plugin-jira");
        fs::create_dir(source.path().join("sem-skill")).unwrap();
        fs::write(source.path().join("solto.md"), "x").unwrap();

        let outcomes = install_skills(source.path(), target.path()).unwrap();

        assert_eq!(
            outcomes,
            vec![SkillOutcome::Linked("mosaic-plugin-jira".to_string())]
        );
    }

    #[test]
    fn missing_source_is_an_error() {
        let base = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let source = base.path().join("nao-existe");

        let error = install_skills(&source, target.path()).unwrap_err();

        assert_eq!(
            error,
            SkillsError::SourceMissing(source.display().to_string())
        );
    }
}
