use std::io::{self, Write};
use std::path::PathBuf;

use rusqlite::Connection;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChunkCoord {
    x: i64,
    y: i64,
    z: i64,
}

#[derive(Copy, Clone)]
enum ChunkCompression {
    Brotli,
}

impl ChunkCompression {
    fn from_int(encoded: u8) -> Option<ChunkCompression> {
        match encoded {
            1 => Some(ChunkCompression::Brotli),
            _ => None,
        }
    }

    fn as_int(self) -> u8 {
        match self {
            ChunkCompression::Brotli => 1,
        }
    }
}

pub struct Region {
    connection: Connection,
}

fn run_schema_create(connection: &Connection) -> rusqlite::Result<()> {
    const RAW_SQL: &str = include_str!("region_file_schema.sql");

    connection.execute_batch(RAW_SQL)?;

    let mime = serde_cbor::to_vec(&"application/vnd.voxel.region.v0").unwrap();
    connection.execute(
        "INSERT INTO metadata VALUES ('mimetype', ?);",
        &[&mime],
    )?;

    Ok(())
}

impl Region {
    pub fn create(path: PathBuf) -> Result<Region, RegionError> {
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE,
        )?;

        run_schema_create(&connection)?;

        Ok(Region { connection })
    }

    pub fn open(path: PathBuf) -> Result<Option<Region>, RegionError> {
        // Ugh, there's a TOCTOU bug here, but the sqlite API doesn't let us avoid it.
        if !path.exists() {
            return Ok(None);
        }

        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        Ok(Some(Region { connection }))
    }

    pub fn load_chunk(&mut self, chunk_coord: &ChunkCoord) -> Result<Vec<u8>, RegionError> {
        let (compression, compressed_chunk_data) = self.connection.query_row(
            "\
SELECT compression, chunk_data
FROM chunks
WHERE
    chunk_x = ?
    AND chunk_y = ?
    AND chunk_z = ?
;
",
            &[chunk_coord.x, chunk_coord.y, chunk_coord.z],
            |row| {
                Ok((
                    ChunkCompression::from_int(row.get_unwrap::<_, u8>(0)).unwrap(),
                    row.get_unwrap::<_, Vec<u8>>(1),
                ))
            },
        )?;

        let chunk_data = match compression {
            ChunkCompression::Brotli => {
                let mut buf = Vec::new();
                {
                    let mut decoder = brotli2::write::BrotliDecoder::new(&mut buf);
                    decoder.write_all(&compressed_chunk_data)?;
                    decoder.finish()?;
                }
                buf
            }
        };
        Ok(chunk_data)
    }

    pub fn save_chunk(
        &mut self,
        chunk_coord: &ChunkCoord,
        chunk_data: &[u8],
    ) -> Result<(), RegionError> {
        let compression = ChunkCompression::Brotli;
        let compressed_data = {
            let mut buf = Vec::new();
            let mut encoder = brotli2::write::BrotliEncoder::new(&mut buf, 11);
            encoder.write_all(&chunk_data)?;
            encoder.finish()?;
            buf
        };

        self.connection.execute("\
INSERT INTO chunks
(chunk_x, chunk_y, chunk_z, compression, chunk_data)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT (chunk_x, chunk_y, chunk_z)
DO UPDATE SET
    compression = excluded.compression,
    chunk_data = excluded.chunk_data
;
",
            rusqlite::params![
                chunk_coord.x,
                chunk_coord.y,
                chunk_coord.z,
                compression.as_int(),
                compressed_data,
            ],
        )?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegionError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("sqlite error: expected 1 row, got {0} while {1}")]
    ExpectedOneRow(i64, &'static str),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
}
