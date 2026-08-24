//! WGSL parsing and validation behind a project-owned public boundary.
//!
//! Naga is deliberately kept out of the public API. Captures and downstream
//! crates can depend on [`ShaderModuleInfo`] and [`ShaderDiagnostic`] without
//! taking a dependency on Naga's IR layout.

use std::fmt;

use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};

/// Shader stages understood by GLGPU's stable shader boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

/// A source range using UTF-8 byte offsets and one-based line/column numbers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub offset: u32,
    pub length: u32,
    pub line: u32,
    pub column: u32,
}

/// Which compiler phase produced a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderDiagnosticKind {
    Parse,
    Validation,
    Unsupported,
}

/// A stable diagnostic suitable for a debugger, capture, or API adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderDiagnostic {
    pub kind: ShaderDiagnosticKind,
    /// Concise message without terminal formatting.
    pub message: String,
    /// Source-oriented message containing the relevant line and caret/labels.
    pub source_message: String,
    pub span: Option<SourceSpan>,
}

impl fmt::Display for ShaderDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source_message)
    }
}

impl std::error::Error for ShaderDiagnostic {}

/// Stable metadata for one WGSL entry point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryPointInfo {
    pub name: String,
    pub stage: ShaderStage,
    /// `[1, 1, 1]` for non-compute stages.
    pub workgroup_size: [u32; 3],
}

/// Stable, serializable-in-spirit metadata extracted from a validated module.
///
/// This initial P0 type intentionally contains only data needed by the API and
/// debugger boundary. It can gain versioned fields without exposing Naga IR.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShaderModuleInfo {
    pub entry_points: Vec<EntryPointInfo>,
}

impl ShaderModuleInfo {
    pub fn entry_point(&self, stage: ShaderStage, name: &str) -> Option<&EntryPointInfo> {
        self.entry_points
            .iter()
            .find(|entry| entry.stage == stage && entry.name == name)
    }
}

/// A validated WGSL module.
///
/// The compiler-owned representation remains private. Consumers use
/// [`Self::info`] today; the shader crate can later lower the private module to
/// GLGPU bytecode without leaking Naga handles into captures or other crates.
pub struct ValidatedShaderModule {
    source: String,
    info: ShaderModuleInfo,
    _module: naga::Module,
    _validation: ModuleInfo,
}

impl fmt::Debug for ValidatedShaderModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedShaderModule")
            .field("source_len", &self.source.len())
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl ValidatedShaderModule {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn info(&self) -> &ShaderModuleInfo {
        &self.info
    }

    pub fn into_info(self) -> ShaderModuleInfo {
        self.info
    }
}

/// Parse and validate a WGSL module with Naga 30.
///
/// P0 intentionally enables no optional Naga capabilities. This validates the
/// core language without accidentally accepting a shader merely because Naga
/// knows a vendor/experimental extension that GLGPU cannot execute.
pub fn parse_and_validate_wgsl(
    source: impl Into<String>,
) -> Result<ValidatedShaderModule, ShaderDiagnostic> {
    parse_and_validate_wgsl_with_label("wgsl", source)
}

/// Parse and validate WGSL, using `label` in the rendered source diagnostic.
pub fn parse_and_validate_wgsl_with_label(
    label: &str,
    source: impl Into<String>,
) -> Result<ValidatedShaderModule, ShaderDiagnostic> {
    let source = source.into();
    let module = naga::front::wgsl::parse_str(&source).map_err(|error| {
        let span = error.location(&source).map(source_span);
        ShaderDiagnostic {
            kind: ShaderDiagnosticKind::Parse,
            message: error.message().to_owned(),
            source_message: error.emit_to_string_with_path(&source, label),
            span,
        }
    })?;

    let validation = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|error| {
            let span = error.location(&source).map(source_span);
            ShaderDiagnostic {
                kind: ShaderDiagnosticKind::Validation,
                message: error.as_inner().to_string(),
                source_message: error.emit_to_string_with_path(&source, label),
                span,
            }
        })?;

    let entry_points = module
        .entry_points
        .iter()
        .map(|entry| {
            let stage = shader_stage(entry.stage).ok_or_else(|| {
                let message = format!(
                    "entry point `{}` uses unsupported Naga stage {:?}",
                    entry.name, entry.stage
                );
                ShaderDiagnostic {
                    kind: ShaderDiagnosticKind::Unsupported,
                    source_message: message.clone(),
                    message,
                    span: None,
                }
            })?;
            Ok(EntryPointInfo {
                name: entry.name.clone(),
                stage,
                workgroup_size: entry.workgroup_size,
            })
        })
        .collect::<Result<Vec<_>, ShaderDiagnostic>>()?;
    let info = ShaderModuleInfo { entry_points };

    Ok(ValidatedShaderModule {
        source,
        info,
        _module: module,
        _validation: validation,
    })
}

fn shader_stage(stage: naga::ShaderStage) -> Option<ShaderStage> {
    match stage {
        naga::ShaderStage::Vertex => Some(ShaderStage::Vertex),
        naga::ShaderStage::Fragment => Some(ShaderStage::Fragment),
        naga::ShaderStage::Compute => Some(ShaderStage::Compute),
        // WGSL does not currently produce the remaining Naga stages. Reject
        // them explicitly rather than silently mapping or panicking if that
        // changes in a future pinned Naga upgrade.
        _ => None,
    }
}

fn source_span(location: naga::SourceLocation) -> SourceSpan {
    SourceSpan {
        offset: location.offset,
        length: location.length,
        line: location.line_number,
        column: location.line_position,
    }
}
