//! Frame data reading from Bruker TDF files.
//!
//! This module reads binary peak data (m/z, intensity, TOF indices) from the
//! `analysis.tdf_bin` file and combines it with frame metadata from the
//! `analysis.tdf` SQLite database. It also automatically detects and loads
//! MALDI imaging metadata when present.
//!
//! # Features
//!
//! - Decompression of Bruker's proprietary TDF binary format
//! - MALDI-TIMS-MSI support with pixel coordinates
//! - Parallel frame reading for performance
//! - DIA window metadata for data-independent acquisition
//! - Full ion mobility (TIMS) data reconstruction
//!
//! # Example
//!
//! ```no_run
//! use timsrust::readers::FrameReader;
//!
//! let reader = FrameReader::new("data.d")?;
//! println!("Total frames: {}", reader.len());
//! println!("Is MALDI imaging: {}", reader.is_maldi());
//!
//! // Get first frame with all data
//! let frame = reader.get(0)?;
//! println!("Peak count: {}", frame.intensities.len());
//!
//! // Check for MALDI metadata
//! if let Some(maldi) = &frame.maldi_info {
//!     println!("Pixel location: ({}, {})", maldi.pixel_x, maldi.pixel_y);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::sync::Arc;

use rayon::iter::{IntoParallelIterator, ParallelIterator};
#[cfg(feature = "timscompress")]
use timscompress::reader::CompressedTdfBlobReader;

use crate::ms_data::{AcquisitionType, Frame, MaldiInfo, MSLevel, QuadrupoleSettings};

use super::{
    file_readers::{
        sql_reader::{
            frame_groups::SqlWindowGroup, frames::SqlFrame, maldi::SqlMaldiFrameInfo,
            ReadableSqlTable, SqlReader, SqlReaderError,
        },
        tdf_blob_reader::{TdfBlob, TdfBlobReader, TdfBlobReaderError},
    },
    MetadataReader, MetadataReaderError, QuadrupoleSettingsReader,
    QuadrupoleSettingsReaderError, TimsTofPathLike,
};

#[derive(Debug)]
pub struct FrameReader {
    tdf_bin_reader: TdfBlobReader,
    #[cfg(feature = "timscompress")]
    compressed_reader: CompressedTdfBlobReader,
    frames: Vec<Frame>,
    acquisition: AcquisitionType,
    offsets: Vec<usize>,
    dia_windows: Option<Vec<Arc<QuadrupoleSettings>>>,
    compression_type: u8,
    #[cfg(feature = "timscompress")]
    scan_count: usize,
    /// Whether this is MALDI imaging data
    is_maldi: bool,
}

impl FrameReader {
    pub fn new(path: impl TimsTofPathLike) -> Result<Self, FrameReaderError> {
        let compression_type =
            match MetadataReader::new(&path)?.compression_type {
                2 => 2,
                #[cfg(feature = "timscompress")]
                3 => 3,
                compression_type => {
                    return Err(FrameReaderError::CompressionTypeError(
                        compression_type,
                    ))
                },
            };

        let tdf_sql_reader = SqlReader::open(&path)?;
        let sql_frames = SqlFrame::from_sql_reader(&tdf_sql_reader)?;
        
        // Load MALDI info if present (for imaging MS data)
        let maldi_info = tdf_sql_reader.read_maldi_frame_info()?;
        let is_maldi = !maldi_info.is_empty();
        let maldi_map: std::collections::HashMap<usize, SqlMaldiFrameInfo> = maldi_info
            .into_iter()
            .map(|m| (m.frame, m))
            .collect();
        
        let tdf_bin_reader = TdfBlobReader::new(&path)?;
        #[cfg(feature = "timscompress")]
        let compressed_reader = CompressedTdfBlobReader::new(&path)
            .ok_or_else(|| FrameReaderError::TimscompressError)?;
        let acquisition = if sql_frames.iter().any(|x| x.msms_type == 8) {
            AcquisitionType::DDAPASEF
        } else if sql_frames.iter().any(|x| x.msms_type == 9) {
            AcquisitionType::DIAPASEF
        } else {
            AcquisitionType::Unknown
        };
        // TODO should be refactored out to quadrupole reader
        let mut window_groups = vec![0; sql_frames.len()];
        let quadrupole_settings;
        if acquisition == AcquisitionType::DIAPASEF {
            for window_group in
                SqlWindowGroup::from_sql_reader(&tdf_sql_reader)?
            {
                window_groups[window_group.frame - 1] =
                    window_group.window_group;
            }
            quadrupole_settings = QuadrupoleSettingsReader::new(&path)?;
        } else {
            quadrupole_settings = vec![];
        }
        // TODO move Arc to quad settings reader?
        let quadrupole_settings = quadrupole_settings
            .into_iter()
            .map(|x| Arc::new(x))
            .collect();
        let frames = (0..sql_frames.len())
            .into_par_iter()
            .map(|index| {
                get_frame_without_data(
                    index,
                    &sql_frames,
                    acquisition,
                    &window_groups,
                    &quadrupole_settings,
                    &maldi_map,
                )
            })
            .collect();
        #[cfg(feature = "timscompress")]
        let scan_count = sql_frames
            .iter()
            .map(|frame| frame.scan_count)
            .max()
            .expect("Frame table cannot be empty")
            as usize;
        let offsets = sql_frames.iter().map(|x| x.binary_offset).collect();
        let reader = Self {
            tdf_bin_reader,
            frames,
            acquisition,
            offsets,
            dia_windows: match acquisition {
                AcquisitionType::DIAPASEF => Some(quadrupole_settings),
                _ => None,
            },
            compression_type,
            #[cfg(feature = "timscompress")]
            compressed_reader,
            #[cfg(feature = "timscompress")]
            scan_count,
            is_maldi,
        };
        Ok(reader)
    }

    // TODO make option result
    pub fn get_binary_offset(&self, index: usize) -> usize {
        self.offsets[index]
    }

    pub fn parallel_filter<'a, F: Fn(&Frame) -> bool + Sync + Send + 'a>(
        &'a self,
        predicate: F,
    ) -> impl ParallelIterator<Item = Result<Frame, FrameReaderError>> + 'a
    {
        (0..self.len())
            .into_par_iter()
            .filter(move |x| predicate(&self.frames[*x]))
            .map(move |x| self.get(x))
    }

    pub fn filter<'a, F: Fn(&Frame) -> bool + Sync + Send + 'a>(
        &'a self,
        predicate: F,
    ) -> impl Iterator<Item = Result<Frame, FrameReaderError>> + 'a {
        (0..self.len())
            .filter(move |x| predicate(&self.frames[*x]))
            .map(move |x| self.get(x))
    }

    pub fn get_dia_windows(&self) -> Option<Vec<Arc<QuadrupoleSettings>>> {
        self.dia_windows.clone()
    }

    pub fn get(&self, index: usize) -> Result<Frame, FrameReaderError> {
        match self.compression_type {
            2 => self.get_from_compression_type_2(index),
            #[cfg(feature = "timscompress")]
            3 => self.get_from_compression_type_3(index),
            _ => Err(FrameReaderError::CompressionTypeError(
                self.compression_type,
            )),
        }
    }

    fn get_from_compression_type_2(
        &self,
        index: usize,
    ) -> Result<Frame, FrameReaderError> {
        // NOTE: get does it by 0-offsetting the vec, not by Frame index!!!
        let mut frame = self.get_frame_without_coordinates(index)?;
        let offset = self.get_binary_offset(index);
        let blob = self.tdf_bin_reader.get(offset)?;
        let scan_count: usize =
            blob.get(0).ok_or(FrameReaderError::CorruptFrame)? as usize;
        let peak_count: usize = (blob.len() - scan_count) / 2;
        frame.scan_offsets = read_scan_offsets(scan_count, peak_count, &blob)?;
        frame.intensities = read_intensities(scan_count, peak_count, &blob)?;
        frame.tof_indices = read_tof_indices(
            scan_count,
            peak_count,
            &blob,
            &frame.scan_offsets,
        )?;
        Ok(frame)
    }

    #[cfg(feature = "timscompress")]
    fn get_from_compression_type_3(
        &self,
        index: usize,
    ) -> Result<Frame, FrameReaderError> {
        // NOTE: get does it by 0-offsetting the vec, not by Frame index!!!
        // TODO
        let mut frame = self.get_frame_without_coordinates(index)?;
        let offset = self.get_binary_offset(index);
        let raw_frame = self
            .compressed_reader
            .get_raw_frame_data(offset, self.scan_count);
        frame.tof_indices = raw_frame.tof_indices;
        frame.intensities = raw_frame.intensities;
        frame.scan_offsets = raw_frame.scan_offsets;
        Ok(frame)
    }

    pub fn get_frame_without_coordinates(
        &self,
        index: usize,
    ) -> Result<Frame, FrameReaderError> {
        let frame = self
            .frames
            .get(index)
            .ok_or(FrameReaderError::IndexOutOfBounds)?
            .clone();
        Ok(frame)
    }

    pub fn get_all(&self) -> Vec<Result<Frame, FrameReaderError>> {
        self.parallel_filter(|_| true).collect()
    }

    pub fn get_all_ms1(&self) -> Vec<Result<Frame, FrameReaderError>> {
        self.parallel_filter(|x| x.ms_level == MSLevel::MS1)
            .collect()
    }

    pub fn get_all_ms2(&self) -> Vec<Result<Frame, FrameReaderError>> {
        self.parallel_filter(|x| x.ms_level == MSLevel::MS2)
            .collect()
    }

    pub fn get_acquisition(&self) -> AcquisitionType {
        self.acquisition
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Returns true if this TDF file contains MALDI imaging data
    pub fn is_maldi(&self) -> bool {
        self.is_maldi
    }
}

fn read_scan_offsets(
    scan_count: usize,
    peak_count: usize,
    blob: &TdfBlob,
) -> Result<Vec<usize>, FrameReaderError> {
    let mut scan_offsets: Vec<usize> = Vec::with_capacity(scan_count + 1);
    scan_offsets.push(0);
    for scan_index in 0..scan_count - 1 {
        let index = scan_index + 1;
        let scan_size: usize =
            (blob.get(index).ok_or(FrameReaderError::CorruptFrame)? / 2)
                as usize;
        scan_offsets.push(scan_offsets[scan_index] + scan_size);
    }
    scan_offsets.push(peak_count);
    Ok(scan_offsets)
}

fn read_intensities(
    scan_count: usize,
    peak_count: usize,
    blob: &TdfBlob,
) -> Result<Vec<u32>, FrameReaderError> {
    let mut intensities: Vec<u32> = Vec::with_capacity(peak_count);
    for peak_index in 0..peak_count {
        let index: usize = scan_count + 1 + 2 * peak_index;
        intensities
            .push(blob.get(index).ok_or(FrameReaderError::CorruptFrame)?);
    }
    Ok(intensities)
}

fn read_tof_indices(
    scan_count: usize,
    peak_count: usize,
    blob: &TdfBlob,
    scan_offsets: &Vec<usize>,
) -> Result<Vec<u32>, FrameReaderError> {
    let mut tof_indices: Vec<u32> = Vec::with_capacity(peak_count);
    for scan_index in 0..scan_count {
        let start_offset: usize = scan_offsets[scan_index];
        let end_offset: usize = scan_offsets[scan_index + 1];
        let mut current_sum: u32 = 0;
        for peak_index in start_offset..end_offset {
            let index = scan_count + 2 * peak_index;
            let tof_index: u32 =
                blob.get(index).ok_or(FrameReaderError::CorruptFrame)?;
            current_sum += tof_index;
            tof_indices.push(current_sum - 1);
        }
    }
    Ok(tof_indices)
}

fn get_frame_without_data(
    index: usize,
    sql_frames: &Vec<SqlFrame>,
    acquisition: AcquisitionType,
    window_groups: &Vec<u8>,
    quadrupole_settings: &Vec<Arc<QuadrupoleSettings>>,
    maldi_map: &std::collections::HashMap<usize, SqlMaldiFrameInfo>,
) -> Frame {
    let mut frame: Frame = Frame::default();
    let sql_frame = &sql_frames[index];
    frame.index = sql_frame.id;
    frame.ms_level = MSLevel::read_from_msms_type(sql_frame.msms_type);
    frame.rt_in_seconds = sql_frame.rt;
    frame.acquisition_type = acquisition;
    frame.intensity_correction_factor = 1.0 / sql_frame.accumulation_time;
    if (acquisition == AcquisitionType::DIAPASEF)
        & (frame.ms_level == MSLevel::MS2)
    {
        // TODO should be refactored out to quadrupole reader
        let window_group = window_groups[index];
        frame.window_group = window_group;
        frame.quadrupole_settings =
            quadrupole_settings[window_group as usize - 1].clone();
    }
    // Attach MALDI info if present (frame IDs are 1-based)
    if let Some(maldi) = maldi_map.get(&sql_frame.id) {
        frame.maldi_info = Some(MaldiInfo {
            spot_name: maldi.spot_name.clone(),
            pixel_x: maldi.x_index_pos,
            pixel_y: maldi.y_index_pos,
            position_x_um: maldi.x_position,
            position_y_um: maldi.y_position,
            laser_power: maldi.laser_power,
            laser_rep_rate: maldi.laser_rep_rate,
            laser_shots: maldi.laser_shots,
        });
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn attaches_maldi_metadata_when_present() {
        let sql_frames = vec![SqlFrame {
            id: 1,
            msms_type: 0,
            rt: 1.5,
            accumulation_time: 100.0,
            ..Default::default()
        }];

        let mut maldi_map = HashMap::new();
        maldi_map.insert(
            1,
            SqlMaldiFrameInfo {
                frame: 1,
                spot_name: "spot-A".to_string(),
                x_index_pos: 10,
                y_index_pos: 20,
                x_position: Some(12.5),
                y_position: Some(25.0),
                laser_power: Some(0.9),
                laser_rep_rate: Some(200.0),
                laser_shots: Some(50),
            },
        );

        let frame = get_frame_without_data(
            0,
            &sql_frames,
            AcquisitionType::DDAPASEF,
            &vec![0],
            &vec![Arc::new(QuadrupoleSettings::default())],
            &maldi_map,
        );

        let maldi = frame.maldi_info.expect("expected MALDI metadata");
        assert_eq!(maldi.spot_name, "spot-A");
        assert_eq!(maldi.pixel_x, 10);
        assert_eq!(maldi.pixel_y, 20);
        assert_eq!(maldi.position_x_um, Some(12.5));
        assert_eq!(maldi.position_y_um, Some(25.0));
        assert_eq!(maldi.laser_power, Some(0.9));
        assert_eq!(maldi.laser_rep_rate, Some(200.0));
        assert_eq!(maldi.laser_shots, Some(50));
        assert_eq!(frame.index, 1);
        assert_eq!(frame.ms_level, MSLevel::MS1);
    }

    #[test]
    fn leaves_maldi_none_when_absent() {
        let sql_frames = vec![SqlFrame {
            id: 2,
            msms_type: 8,
            rt: 2.0,
            accumulation_time: 50.0,
            ..Default::default()
        }];

        let frame = get_frame_without_data(
            0,
            &sql_frames,
            AcquisitionType::DDAPASEF,
            &vec![0],
            &vec![Arc::new(QuadrupoleSettings::default())],
            &HashMap::new(),
        );

        assert!(frame.maldi_info.is_none());
        assert_eq!(frame.index, 2);
        assert_eq!(frame.ms_level, MSLevel::MS2);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameReaderError {
    #[cfg(feature = "timscompress")]
    #[error("Timscompress error")]
    TimscompressError,
    #[error("{0}")]
    TdfBlobReaderError(#[from] TdfBlobReaderError),
    #[error("{0}")]
    MetadataReaderError(#[from] MetadataReaderError),
    #[error("{0}")]
    FileNotFound(String),
    #[error("{0}")]
    SqlReaderError(#[from] SqlReaderError),
    #[error("Corrupt Frame")]
    CorruptFrame,
    #[error("{0}")]
    QuadrupoleSettingsReaderError(#[from] QuadrupoleSettingsReaderError),
    #[error("Index out of bounds")]
    IndexOutOfBounds,
    #[error("Compression type {0} not understood")]
    CompressionTypeError(u8),
}
