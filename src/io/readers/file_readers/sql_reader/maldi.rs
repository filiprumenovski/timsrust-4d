//! MALDI frame information from Bruker TDF files.
//!
//! This module reads metadata from the `MaldiFrameInfo` table in Bruker TimsTOF
//! data files, providing spatial coordinates and laser parameters for MALDI imaging
//! mass spectrometry (MALDI-TIMS-MSI) experiments.
//!
//! # Overview
//!
//! MALDI imaging data includes spatial coordinates (pixel grid position and physical
//! coordinates) along with laser settings used for each acquired pixel/frame.
//!
//! # Example
//!
//! ```no_run
//! use timsrust::io::readers::file_readers::sql_reader::SqlReader;
//!
//! let reader = SqlReader::open("analysis.tdf")?;
//! if reader.has_maldi_info() {
//!     let maldi_frames = reader.read_maldi_frame_info()?;
//!     for frame_info in maldi_frames {
//!         println!("Frame {}: pixel ({}, {}), spot: {}",
//!                  frame_info.frame,
//!                  frame_info.x_index_pos,
//!                  frame_info.y_index_pos,
//!                  frame_info.spot_name);
//!     }
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use super::{ParseDefault, ReadableSqlTable, SqlReader, SqlReaderError};

/// MALDI frame information from MaldiFrameInfo table.
/// Contains spatial coordinates for imaging mass spectrometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SqlMaldiFrameInfo {
    /// Frame ID (corresponds to Frame.Id)
    pub frame: usize,
    /// Spot name identifier
    pub spot_name: String,
    /// X pixel coordinate (grid index)
    pub x_index_pos: i32,
    /// Y pixel coordinate (grid index)
    pub y_index_pos: i32,
    /// X position in micrometers (physical coordinate)
    pub x_position: Option<f64>,
    /// Y position in micrometers (physical coordinate)
    pub y_position: Option<f64>,
    /// Laser power setting
    pub laser_power: Option<f64>,
    /// Laser repetition rate
    pub laser_rep_rate: Option<f64>,
    /// Number of laser shots
    pub laser_shots: Option<i32>,
}

impl ReadableSqlTable for SqlMaldiFrameInfo {
    fn get_sql_query() -> String {
        "SELECT Frame, SpotName, XIndexPos, YIndexPos, PositionX, PositionY, \
         LaserPower, LaserRepRate, NumLaserShots FROM MaldiFrameInfo"
            .to_string()
    }

    fn from_sql_row(row: &rusqlite::Row) -> Self {
        Self {
            frame: row.parse_default(0),
            spot_name: row.get(1).unwrap_or_default(),
            x_index_pos: row.parse_default(2),
            y_index_pos: row.parse_default(3),
            x_position: row.get(4).ok(),
            y_position: row.get(5).ok(),
            laser_power: row.get(6).ok(),
            laser_rep_rate: row.get(7).ok(),
            laser_shots: row.get(8).ok(),
        }
    }
}

impl SqlReader {
    /// Check if this TDF file contains MALDI imaging data by checking
    /// for the MaldiFrameInfo table.
    pub fn has_maldi_info(&self) -> bool {
        let query =
            "SELECT name FROM sqlite_master WHERE type='table' AND name='MaldiFrameInfo'";
        self.connection
            .prepare(query)
            .and_then(|mut stmt| stmt.query_row([], |_| Ok(true)))
            .unwrap_or(false)
    }

    /// Read all MALDI frame info entries.
    /// Returns an empty Vec if the table doesn't exist.
    pub fn read_maldi_frame_info(
        &self,
    ) -> Result<Vec<SqlMaldiFrameInfo>, SqlReaderError> {
        if !self.has_maldi_info() {
            return Ok(Vec::new());
        }
        SqlMaldiFrameInfo::from_sql_reader(self)
    }
}

/// MALDI-specific metadata attached to a frame for imaging MS.
///
/// This struct is automatically constructed from `SqlMaldiFrameInfo` and
/// attached to frames during frame reading. It contains spatial coordinates
/// and laser parameters for MALDI imaging datasets.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub struct MaldiFrameInfo {
    /// Spot name identifier
    pub spot_name: String,
    /// X pixel coordinate (grid index)
    pub pixel_x: i32,
    /// Y pixel coordinate (grid index)
    pub pixel_y: i32,
    /// X position in micrometers (physical coordinate)
    pub position_x_um: Option<f64>,
    /// Y position in micrometers (physical coordinate)
    pub position_y_um: Option<f64>,
    /// Laser power setting
    pub laser_power: Option<f64>,
    /// Laser repetition rate
    pub laser_rep_rate: Option<f64>,
    /// Number of laser shots
    pub laser_shots: Option<i32>,
}

impl From<SqlMaldiFrameInfo> for MaldiFrameInfo {
    fn from(sql: SqlMaldiFrameInfo) -> Self {
        Self {
            spot_name: sql.spot_name,
            pixel_x: sql.x_index_pos,
            pixel_y: sql.y_index_pos,
            position_x_um: sql.x_position,
            position_y_um: sql.y_position,
            laser_power: sql.laser_power,
            laser_rep_rate: sql.laser_rep_rate,
            laser_shots: sql.laser_shots,
        }
    }
}
