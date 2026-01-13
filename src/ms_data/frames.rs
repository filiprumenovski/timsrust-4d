use super::{AcquisitionType, QuadrupoleSettings};
use std::sync::Arc;

/// MALDI-specific metadata attached to a frame for imaging MS.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaldiInfo {
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

/// A frame with all unprocessed data as it was acquired.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub scan_offsets: Vec<usize>,
    pub tof_indices: Vec<u32>,
    pub intensities: Vec<u32>,
    pub index: usize,
    pub rt_in_seconds: f64,
    pub acquisition_type: AcquisitionType,
    pub ms_level: MSLevel,
    pub quadrupole_settings: Arc<QuadrupoleSettings>,
    pub intensity_correction_factor: f64,
    pub window_group: u8,
    /// MALDI imaging metadata (only present for MALDI-TIMS-MSI data)
    pub maldi_info: Option<MaldiInfo>,
}

impl Frame {
    pub fn get_corrected_intensity(&self, index: usize) -> f64 {
        self.intensity_correction_factor * self.intensities[index] as f64
    }
}

/// The MS level used.
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum MSLevel {
    MS1,
    MS2,
    /// Default value.
    #[default]
    Unknown,
}

impl MSLevel {
    pub fn read_from_msms_type(msms_type: u8) -> MSLevel {
        match msms_type {
            0 => MSLevel::MS1,
            8 => MSLevel::MS2,
            9 => MSLevel::MS2,
            _ => MSLevel::Unknown,
        }
    }
}
