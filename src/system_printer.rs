use std::{
    io::Write,
    process::{Command, Stdio},
};

use thiserror::Error;

use crate::config::AgentConfig;

#[derive(Debug, Error)]
pub enum SystemPrinterError {
    #[error("No system printer found. Install or select a default printer in the OS.")]
    NotFound,

    #[error("Failed to start print command: {0}")]
    Command(#[from] std::io::Error),

    #[error("Print command failed: {0}")]
    CommandFailed(String),
}

#[derive(Clone)]
pub struct SystemPrinterManager {
    config: AgentConfig,
}

impl SystemPrinterManager {
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    pub fn list(&self) -> Vec<String> {
        list_system_printers().unwrap_or_default()
    }

    pub fn print(&self, data: &[u8]) -> Result<(), SystemPrinterError> {
        print_with_system_backend(data, self.config.system_printer_name.as_deref())
    }
}

fn is_virtual_printer_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    [
        "microsoft print to pdf",
        "microsoft xps document writer",
        "fax",
        "onenote",
        "send to onenote",
    ]
    .iter()
    .any(|virtual_name| normalized.contains(virtual_name))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn list_system_printers() -> Result<Vec<String>, SystemPrinterError> {
    let output = Command::new("lpstat").arg("-p").output()?;
    if !output.status.success() {
        return Err(SystemPrinterError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let printers = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("printer ")?;
            Some(rest.split_whitespace().next()?.to_string())
        })
        .filter(|name| !is_virtual_printer_name(name))
        .collect::<Vec<_>>();

    if printers.is_empty() {
        return Err(SystemPrinterError::NotFound);
    }

    Ok(printers)
}

#[cfg(target_os = "windows")]
fn list_system_printers() -> Result<Vec<String>, SystemPrinterError> {
    let script = "Get-Printer | Select-Object -ExpandProperty Name";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()?;

    if !output.status.success() {
        return Err(SystemPrinterError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let printers = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !is_virtual_printer_name(line))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if printers.is_empty() {
        return Err(SystemPrinterError::NotFound);
    }

    Ok(printers)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn list_system_printers() -> Result<Vec<String>, SystemPrinterError> {
    Err(SystemPrinterError::CommandFailed(
        "unsupported print backend on this platform".into(),
    ))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn print_with_system_backend(
    data: &[u8],
    printer_name: Option<&str>,
) -> Result<(), SystemPrinterError> {
    let mut cmd = Command::new("lp");
    cmd.arg("-o").arg("raw");

    if let Some(printer_name) = printer_name {
        cmd.arg("-d").arg(printer_name);
    }

    let mut child = cmd.stdin(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(data)?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(SystemPrinterError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn print_with_system_backend(
    data: &[u8],
    printer_name: Option<&str>,
) -> Result<(), SystemPrinterError> {
    let printer_expr = match printer_name {
        Some(name) => format!("'{}'", name.replace('\'', "''")),
        None => "(Get-CimInstance Win32_Printer | Where-Object Default | Select-Object -First 1 -ExpandProperty Name)".to_string(),
    };

    let bytes = data
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let script = format!(
        r#"
$printerName = {printer_expr}
if ([string]::IsNullOrWhiteSpace($printerName)) {{ throw 'No default printer configured' }}
$code = @"
using System;
using System.Runtime.InteropServices;
public static class RawPrinterHelper {{
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
  public class DOCINFO {{
    [MarshalAs(UnmanagedType.LPWStr)] public string pDocName;
    [MarshalAs(UnmanagedType.LPWStr)] public string pOutputFile;
    [MarshalAs(UnmanagedType.LPWStr)] public string pDataType;
  }}
  [DllImport("winspool.drv", EntryPoint="OpenPrinterW", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern bool OpenPrinter(string pPrinterName, out IntPtr hPrinter, IntPtr pDefault);
  [DllImport("winspool.drv", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern bool ClosePrinter(IntPtr hPrinter);
  [DllImport("winspool.drv", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern bool StartDocPrinter(IntPtr hPrinter, int level, DOCINFO di);
  [DllImport("winspool.drv", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern bool EndDocPrinter(IntPtr hPrinter);
  [DllImport("winspool.drv", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern bool StartPagePrinter(IntPtr hPrinter);
  [DllImport("winspool.drv", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern bool EndPagePrinter(IntPtr hPrinter);
  [DllImport("winspool.drv", SetLastError=true)]
  public static extern bool WritePrinter(IntPtr hPrinter, byte[] pBytes, int dwCount, out int dwWritten);
}}
"@
Add-Type -TypeDefinition $code
$bytes = [byte[]]@({bytes})
$doc = New-Object RawPrinterHelper+DOCINFO
$doc.pDocName = 'efact receipt'
$doc.pDataType = 'RAW'
$handle = [IntPtr]::Zero
if (-not [RawPrinterHelper]::OpenPrinter($printerName, [ref]$handle, [IntPtr]::Zero)) {{ throw 'OpenPrinter failed' }}
try {{
  if (-not [RawPrinterHelper]::StartDocPrinter($handle, 1, $doc)) {{ throw 'StartDocPrinter failed' }}
  try {{
    if (-not [RawPrinterHelper]::StartPagePrinter($handle)) {{ throw 'StartPagePrinter failed' }}
    try {{
      $written = 0
      if (-not [RawPrinterHelper]::WritePrinter($handle, $bytes, $bytes.Length, [ref]$written)) {{ throw 'WritePrinter failed' }}
      if ($written -ne $bytes.Length) {{ throw "Partial write: $written / $($bytes.Length)" }}
    }} finally {{ [void][RawPrinterHelper]::EndPagePrinter($handle) }}
  }} finally {{ [void][RawPrinterHelper]::EndDocPrinter($handle) }}
}} finally {{ [void][RawPrinterHelper]::ClosePrinter($handle) }}
"#,
        printer_expr = printer_expr,
        bytes = bytes,
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()?;

    if !output.status.success() {
        return Err(SystemPrinterError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn print_with_system_backend(
    _data: &[u8],
    _printer_name: Option<&str>,
) -> Result<(), SystemPrinterError> {
    Err(SystemPrinterError::CommandFailed(
        "unsupported print backend on this platform".into(),
    ))
}
