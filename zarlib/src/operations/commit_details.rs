use crate::MetadataType;
use std::collections::HashMap;

pub(crate) const COMMIT_NAME_KEY: &str = "zar.commit.name";
pub(crate) const COMMIT_DESCRIPTION_KEY: &str = "zar.commit.description";

/// Optional name and description to store on a newly created commit.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommitDetails<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// A field can be left unchanged, set, or removed.
#[derive(Debug, Clone, Copy, Default)]
pub enum CommitField<'a> {
    #[default]
    Unchanged,
    Set(&'a str),
    Remove,
}

/// Changes to the name and description of an existing commit.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommitDetailsUpdate<'a> {
    pub name: CommitField<'a>,
    pub description: CommitField<'a>,
}

impl CommitDetails<'_> {
    pub(crate) fn apply(
        self,
        metadata: &mut HashMap<String, MetadataType>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        apply_update(
            metadata,
            CommitDetailsUpdate {
                name: self.name.map_or(CommitField::Unchanged, CommitField::Set),
                description: self
                    .description
                    .map_or(CommitField::Unchanged, CommitField::Set),
            },
        )
    }
}

pub(crate) fn apply_update(
    metadata: &mut HashMap<String, MetadataType>,
    update: CommitDetailsUpdate<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let CommitField::Set(name) = update.name
        && (name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.'))
    {
        return Err(
            "commit name must be 1-64 ASCII letters, digits, underscores, or periods".into(),
        );
    }
    if let CommitField::Set(description) = update.description
        && description.chars().count() > 1024
    {
        return Err("commit description exceeds 1024 characters".into());
    }
    for (key, field) in [
        (COMMIT_NAME_KEY, update.name),
        (COMMIT_DESCRIPTION_KEY, update.description),
    ] {
        match field {
            CommitField::Unchanged => {}
            CommitField::Set(value) => {
                metadata.insert(key.to_owned(), MetadataType::String(value.to_owned()));
            }
            CommitField::Remove => {
                metadata.remove(key);
            }
        }
    }
    Ok(())
}
