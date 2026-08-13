//! Tags — CONCEPTION.md decision #20, filtering on the library screen.

use rusqlite::Connection;

use super::{Database, DbError};

/// A tag and how many transcripts carry it, for the sidebar list of DESIGN.md §7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub name: String,
    pub count: u32,
}

impl Database {
    /// Replaces the tags of a transcript.
    pub fn set_tags(&self, transcript_id: &str, tags: &[String]) -> Result<(), DbError> {
        let tx = self.conn().unchecked_transaction()?;
        Self::write_tags(&tx, transcript_id, tags)?;
        tx.commit()?;
        Ok(())
    }

    /// The tag half of a save, sharing the caller's transaction.
    pub(crate) fn write_tags(
        conn: &Connection,
        transcript_id: &str,
        tags: &[String],
    ) -> Result<(), DbError> {
        conn.execute(
            "DELETE FROM transcript_tags WHERE transcript_id = ?1",
            [transcript_id],
        )?;

        for name in tags {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }

            // The tag table is shared, so a tag keeps its identifier across transcripts and
            // renaming one later is a single row.
            conn.execute(
                "INSERT INTO tags (name) VALUES (?1) ON CONFLICT (name) DO NOTHING",
                [name],
            )?;
            conn.execute(
                "INSERT INTO transcript_tags (transcript_id, tag_id)
                 SELECT ?1, id FROM tags WHERE name = ?2
                 ON CONFLICT DO NOTHING",
                (transcript_id, name),
            )?;
        }

        Ok(())
    }

    /// Tags of one transcript, in alphabetical order.
    pub fn tags_of(&self, transcript_id: &str) -> Result<Vec<String>, DbError> {
        let mut statement = self.conn().prepare(
            "SELECT t.name FROM tags t
             JOIN transcript_tags tt ON tt.tag_id = t.id
             WHERE tt.transcript_id = ?1
             ORDER BY t.name",
        )?;
        let tags = statement
            .query_map([transcript_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    /// Every tag in use, with its count. Tags nobody carries are left out.
    pub fn all_tags(&self) -> Result<Vec<TagCount>, DbError> {
        let mut statement = self.conn().prepare(
            "SELECT t.name, count(tt.transcript_id) AS n FROM tags t
             JOIN transcript_tags tt ON tt.tag_id = t.id
             GROUP BY t.id
             ORDER BY t.name",
        )?;
        let tags = statement
            .query_map([], |row| {
                Ok(TagCount {
                    name: row.get(0)?,
                    count: row.get::<_, i64>(1)?.max(0) as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    /// Identifiers carrying a given tag — the filter of DESIGN.md §7.
    pub fn transcripts_with_tag(&self, tag: &str) -> Result<Vec<String>, DbError> {
        let mut statement = self.conn().prepare(
            "SELECT tt.transcript_id FROM transcript_tags tt
             JOIN tags t ON t.id = tt.tag_id
             JOIN transcripts tr ON tr.id = tt.transcript_id
             WHERE t.name = ?1
             ORDER BY tr.created_at DESC",
        )?;
        let ids = statement
            .query_map([tag], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::transcripts::tests::transcript;

    fn db() -> Database {
        let db = Database::open_in_memory().expect("open");
        db.save_transcript(&transcript("a", "Une")).expect("save");
        db.save_transcript(&transcript("b", "Deux")).expect("save");
        db
    }

    #[test]
    fn tags_are_replaced_wholesale_and_read_back_sorted() {
        let db = db();
        db.set_tags("a", &["réunion".to_string(), "client".to_string()])
            .expect("set");
        assert_eq!(db.tags_of("a").expect("read"), vec!["client", "réunion"]);

        db.set_tags("a", &["client".to_string()]).expect("replace");
        assert_eq!(db.tags_of("a").expect("read"), vec!["client"]);
    }

    #[test]
    fn the_same_tag_on_two_transcripts_is_one_row() {
        let db = db();
        db.set_tags("a", &["client".to_string()]).expect("set");
        db.set_tags("b", &["client".to_string()]).expect("set");

        assert_eq!(
            db.all_tags().expect("all"),
            vec![TagCount {
                name: "client".to_string(),
                count: 2,
            }]
        );
    }

    #[test]
    fn blank_tags_are_dropped_rather_than_stored() {
        let db = db();
        db.set_tags("a", &["  ".to_string(), " client ".to_string()])
            .expect("set");
        assert_eq!(db.tags_of("a").expect("read"), vec!["client"]);
    }

    #[test]
    fn the_same_tag_twice_on_one_transcript_is_kept_once() {
        let db = db();
        db.set_tags("a", &["client".to_string(), "client".to_string()])
            .expect("set");
        assert_eq!(db.tags_of("a").expect("read"), vec!["client"]);
    }

    #[test]
    fn filtering_by_tag_lists_the_right_transcripts() {
        let db = db();
        db.set_tags("a", &["client".to_string()]).expect("set");
        db.set_tags("b", &["perso".to_string()]).expect("set");

        assert_eq!(db.transcripts_with_tag("client").expect("filter"), ["a"]);
        assert_eq!(db.transcripts_with_tag("perso").expect("filter"), ["b"]);
        assert!(db
            .transcripts_with_tag("absent")
            .expect("filter")
            .is_empty());
    }

    #[test]
    fn deleting_a_transcript_releases_its_tags() {
        let db = db();
        db.set_tags("a", &["client".to_string()]).expect("set");
        db.set_tags("b", &["client".to_string()]).expect("set");

        db.delete_transcript("a").expect("delete");
        assert_eq!(
            db.all_tags().expect("all"),
            vec![TagCount {
                name: "client".to_string(),
                count: 1,
            }],
            "the tag stays, its count drops"
        );

        db.delete_transcript("b").expect("delete");
        assert!(
            db.all_tags().expect("all").is_empty(),
            "a tag nobody carries is no longer listed"
        );
    }

    #[test]
    fn saving_a_transcript_carries_its_tags_along() {
        let db = Database::open_in_memory().expect("open");
        let mut fixture = transcript("a", "Une");
        fixture.tags = vec!["client".to_string(), "réunion".to_string()];
        db.save_transcript(&fixture).expect("save");

        assert_eq!(db.tags_of("a").expect("read"), vec!["client", "réunion"]);
    }
}
