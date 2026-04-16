use hidapi::{HidApi, HidDevice};
use thiserror::Error;

use crate::config::AgentConfig;

/// Known thermal printer USB vendor IDs (decimal).
/// Covers Epson, Star Micronics, Bixolon, Citizen, Sewoo, Rongta, Xprinter.
const KNOWN_VENDOR_IDS: &[u16] = &[
    0x04B8, // Epson
    0x0519, // Star Micronics
    0x1504, // Bixolon
    0x1CBE, // Citizen
    0x0DD4, // Custom / Sewoo
    0x20D1, // Rongta
    0x0FE6, // ICS / Xprinter
    0x6868, // Xprinter (alternate)
];

#[derive(Debug, Error)]
pub enum PrinterError {
    #[error("No USB thermal printer found. Check USB connection and config.toml.")]
    NotFound,

    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("Write error: wrote {written} of {total} bytes")]
    IncompleteWrite { written: usize, total: usize },
}

pub struct PrinterManager {
    config: AgentConfig,
}

impl PrinterManager {
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    /// Returns a list of detected thermal printer names for the /printers endpoint.
    pub fn list(&self) -> Vec<String> {
        match HidApi::new() {
            Err(_) => vec![],
            Ok(api) => api
                .device_list()
                .filter(|d| self.is_target_device(d.vendor_id(), d.product_id()))
                .map(|d| {
                    format!(
                        "{} (VID:{:04X} PID:{:04X})",
                        d.product_string().unwrap_or("Unknown"),
                        d.vendor_id(),
                        d.product_id(),
                    )
                })
                .collect(),
        }
    }

    /// Opens the first matching device and writes all ESC/POS bytes to it.
    pub fn print(&self, data: &[u8]) -> Result<(), PrinterError> {
        let api = HidApi::new()?;

        let device = self.open_device(&api)?;
        self.write_all(&device, data)?;

        tracing::info!("Printed {} bytes successfully", data.len());
        Ok(())
    }

    // ── private ──────────────────────────────────────────────────────────────

    fn open_device(&self, api: &HidApi) -> Result<HidDevice, PrinterError> {
        // If explicit VID+PID are configured, use them directly.
        if let (Some(vid_str), Some(pid_str)) =
            (&self.config.usb_vendor_id, &self.config.usb_product_id)
        {
            let vid = parse_hex_id(vid_str);
            let pid = parse_hex_id(pid_str);
            return api.open(vid, pid).map_err(|_| PrinterError::NotFound);
        }

        // Otherwise scan all connected HID devices for known thermal printers.
        for info in api.device_list() {
            if self.is_target_device(info.vendor_id(), info.product_id()) {
                if let Ok(device) = info.open_device(api) {
                    tracing::info!(
                        "Opened printer: {} (VID:{:04X} PID:{:04X})",
                        info.product_string().unwrap_or("Unknown"),
                        info.vendor_id(),
                        info.product_id(),
                    );
                    return Ok(device);
                }
            }
        }

        Err(PrinterError::NotFound)
    }

    /// Writes `data` in chunks. HID `write()` prepends a report-ID byte (0x00)
    /// so each chunk must be <= chunk_size - 1 payload bytes.
    fn write_all(&self, device: &HidDevice, data: &[u8]) -> Result<(), PrinterError> {
        let chunk_size = self.config.chunk_size.saturating_sub(1).max(1);
        let mut total_written = 0usize;

        for chunk in data.chunks(chunk_size) {
            // HID write buffer: [report_id=0x00, payload...]
            let mut buf = Vec::with_capacity(chunk.len() + 1);
            buf.push(0x00);
            buf.extend_from_slice(chunk);

            let written = device.write(&buf)?;
            // written includes the report-ID byte
            total_written += written.saturating_sub(1);
        }

        if total_written < data.len() {
            return Err(PrinterError::IncompleteWrite {
                written: total_written,
                total: data.len(),
            });
        }

        Ok(())
    }

    fn is_target_device(&self, vid: u16, _pid: u16) -> bool {
        if let Some(target_vid) = &self.config.usb_vendor_id {
            return vid == parse_hex_id(target_vid);
        }
        KNOWN_VENDOR_IDS.contains(&vid)
    }
}

fn parse_hex_id(s: &str) -> u16 {
    u16::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}
