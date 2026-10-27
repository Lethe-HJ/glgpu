//! Self-authored bytecode and a CPU interpreter.
//!
//! Lowering is `ESSL AST -> bytecode`. Execution walks that bytecode and keeps
//! a source span plus the local snapshot after every instruction. There is no
//! machine-code JIT. A single-step debugger can later present `DebugStep`
//! without changing the instruction format.

use std::error::Error;
use std::fmt;

use crate::essl::{Expr, FrontendError, Function, SourceSpan, Statement, TranslationUnit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Uniform,
    ShaderIn,
    ShaderOut,
    Local,
}

/// A named slot retained so a later debugger can show uniforms, inputs, outputs, and locals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub op: Op,
    pub span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Op {
    /// Write an f32 bit pattern into a local slot.
    LoadConst { dest: u16, value_bits: u32 },
    /// Copy one local slot into another.
    Move { dest: u16, src: u16 },
}

impl Op {
    pub const fn name(self) -> &'static str {
        match self {
            Self::LoadConst { .. } => "load_const",
            Self::Move { .. } => "move",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytecodeFunction {
    pub name: String,
    pub span: SourceSpan,
    pub locals: Vec<DebugSymbol>,
    pub instructions: Vec<Instruction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytecodeModule {
    pub uniforms: Vec<DebugSymbol>,
    pub inputs: Vec<DebugSymbol>,
    pub outputs: Vec<DebugSymbol>,
    pub functions: Vec<BytecodeFunction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowerError {
    pub message: String,
    pub span: SourceSpan,
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (bytes {}..{})",
            self.message, self.span.start, self.span.end
        )
    }
}

impl Error for LowerError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmError {
    pub message: String,
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for VmError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSnapshot {
    pub name: String,
    pub value_bits: Option<u32>,
}

impl LocalSnapshot {
    pub fn value(self) -> Option<f32> {
        self.value_bits.map(f32::from_bits)
    }
}

/// One interpreter step. `span` points back at the ESSL statement that produced `op`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugStep {
    pub pc: usize,
    pub span: SourceSpan,
    pub op: &'static str,
    pub locals: Vec<LocalSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecTrace {
    pub steps: Vec<DebugStep>,
}

struct LocalBuilder {
    name: String,
    span: SourceSpan,
    assigned: bool,
}

/// Lower every function in the subset AST.
///
/// Interface variables are kept on the module for a future debugger. Reading
/// or writing them is outside this subset, as are constructors and built-ins
/// such as `gl_Position`.
pub fn lower(unit: &TranslationUnit) -> Result<BytecodeModule, LowerError> {
    let uniforms = interface_symbols(&unit.uniforms, SymbolKind::Uniform);
    let inputs = interface_symbols(&unit.inputs, SymbolKind::ShaderIn);
    let outputs = interface_symbols(&unit.outputs, SymbolKind::ShaderOut);
    let mut functions = Vec::with_capacity(unit.functions.len());
    for function in &unit.functions {
        functions.push(lower_function(function, &uniforms, &inputs, &outputs)?);
    }
    Ok(BytecodeModule {
        uniforms,
        inputs,
        outputs,
        functions,
    })
}

fn interface_symbols(
    symbols: &[crate::essl::InterfaceSymbol],
    kind: SymbolKind,
) -> Vec<DebugSymbol> {
    symbols
        .iter()
        .map(|symbol| DebugSymbol {
            name: symbol.name.clone(),
            kind,
            span: symbol.span,
        })
        .collect()
}

fn lower_function(
    function: &Function,
    uniforms: &[DebugSymbol],
    inputs: &[DebugSymbol],
    outputs: &[DebugSymbol],
) -> Result<BytecodeFunction, LowerError> {
    let mut locals = Vec::<LocalBuilder>::new();
    let mut instructions = Vec::new();
    for statement in &function.body {
        match statement {
            Statement::Declare { name, value, span } => {
                let dest = push_local(&mut locals, name, *span)?;
                let op = lower_value(value, &locals, uniforms, inputs, outputs, dest)?;
                locals[dest as usize].assigned = true;
                instructions.push(Instruction { op, span: *span });
            }
            Statement::Assign { name, value, span } => {
                let dest = local_slot(&locals, name).ok_or_else(|| {
                    lower_error(
                        if interface_named(name, uniforms, inputs, outputs) {
                            format!("cannot assign interface variable `{name}` in this subset")
                        } else {
                            format!("unknown local `{name}`")
                        },
                        *span,
                    )
                })?;
                let op = lower_value(value, &locals, uniforms, inputs, outputs, dest)?;
                locals[dest as usize].assigned = true;
                instructions.push(Instruction { op, span: *span });
            }
        }
    }
    Ok(BytecodeFunction {
        name: function.name.clone(),
        span: function.span,
        locals: locals
            .into_iter()
            .map(|local| DebugSymbol {
                name: local.name,
                kind: SymbolKind::Local,
                span: local.span,
            })
            .collect(),
        instructions,
    })
}

fn push_local(
    locals: &mut Vec<LocalBuilder>,
    name: &str,
    span: SourceSpan,
) -> Result<u16, LowerError> {
    let slot = u16::try_from(locals.len())
        .map_err(|_| lower_error("too many locals for the learning bytecode", span))?;
    locals.push(LocalBuilder {
        name: name.to_string(),
        span,
        assigned: false,
    });
    Ok(slot)
}

fn lower_value(
    value: &Expr,
    locals: &[LocalBuilder],
    uniforms: &[DebugSymbol],
    inputs: &[DebugSymbol],
    outputs: &[DebugSymbol],
    dest: u16,
) -> Result<Op, LowerError> {
    match value {
        Expr::Float { bits, .. } => Ok(Op::LoadConst {
            dest,
            value_bits: *bits,
        }),
        Expr::Name { name, span } => {
            if interface_named(name, uniforms, inputs, outputs) {
                return Err(lower_error(
                    format!("reading interface variable `{name}` is outside this subset"),
                    *span,
                ));
            }
            let src = local_slot(locals, name)
                .ok_or_else(|| lower_error(format!("unknown local `{name}`"), *span))?;
            if !locals[src as usize].assigned {
                return Err(lower_error(
                    format!("local `{name}` is read before it is assigned"),
                    *span,
                ));
            }
            Ok(Op::Move { dest, src })
        }
    }
}

fn local_slot(locals: &[LocalBuilder], name: &str) -> Option<u16> {
    locals
        .iter()
        .position(|local| local.name == name)
        .map(|index| index as u16)
}

fn interface_named(
    name: &str,
    uniforms: &[DebugSymbol],
    inputs: &[DebugSymbol],
    outputs: &[DebugSymbol],
) -> bool {
    uniforms
        .iter()
        .chain(inputs)
        .chain(outputs)
        .any(|symbol| symbol.name == name)
}

fn lower_error(message: impl Into<String>, span: SourceSpan) -> LowerError {
    LowerError {
        message: message.into(),
        span,
    }
}

/// Interpret one lowered function on the CPU.
///
/// Locals start unassigned. Each step records the source span and the local
/// slots after that instruction. Uniforms and varyings are not given storage
/// until a later subset can read them.
pub fn execute(module: &BytecodeModule, function_name: &str) -> Result<ExecTrace, VmError> {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == function_name)
        .ok_or_else(|| VmError {
            message: format!("unknown function `{function_name}`"),
        })?;
    let mut values = vec![None; function.locals.len()];
    let mut steps = Vec::with_capacity(function.instructions.len());
    for (pc, instruction) in function.instructions.iter().enumerate() {
        match instruction.op {
            Op::LoadConst { dest, value_bits } => {
                let slot = values.get_mut(dest as usize).ok_or_else(|| VmError {
                    message: format!("local slot {dest} is missing"),
                })?;
                *slot = Some(value_bits);
            }
            Op::Move { dest, src } => {
                let value = values
                    .get(src as usize)
                    .copied()
                    .flatten()
                    .ok_or_else(|| VmError {
                        message: format!("local slot {src} is unassigned"),
                    })?;
                let slot = values.get_mut(dest as usize).ok_or_else(|| VmError {
                    message: format!("local slot {dest} is missing"),
                })?;
                *slot = Some(value);
            }
        }
        steps.push(DebugStep {
            pc,
            span: instruction.span,
            op: instruction.op.name(),
            locals: function
                .locals
                .iter()
                .zip(values.iter())
                .map(|(symbol, value)| LocalSnapshot {
                    name: symbol.name.clone(),
                    value_bits: *value,
                })
                .collect(),
        });
    }
    Ok(ExecTrace { steps })
}

/// Parse, lower, and interpret the subset. The frontend error is returned unchanged.
pub fn compile_and_execute(source: &str, function_name: &str) -> Result<ExecTrace, CompileError> {
    let unit = crate::essl::parse_essl_subset(source)?;
    let module = lower(&unit)?;
    Ok(execute(&module, function_name)?)
}

#[derive(Debug)]
pub enum CompileError {
    Frontend(FrontendError),
    Lower(LowerError),
    Vm(VmError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frontend(error) => write!(f, "{error}"),
            Self::Lower(error) => write!(f, "{error}"),
            Self::Vm(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CompileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frontend(error) => Some(error),
            Self::Lower(error) => Some(error),
            Self::Vm(error) => Some(error),
        }
    }
}

impl From<FrontendError> for CompileError {
    fn from(error: FrontendError) -> Self {
        Self::Frontend(error)
    }
}

impl From<LowerError> for CompileError {
    fn from(error: LowerError) -> Self {
        Self::Lower(error)
    }
}

impl From<VmError> for CompileError {
    fn from(error: VmError) -> Self {
        Self::Vm(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essl::parse_essl_subset;

    const SAMPLE: &str = include_str!("../../../examples/webgl2-triangle/float_local.essl");

    #[test]
    fn lowers_and_steps_the_float_local_fixture() {
        let unit = parse_essl_subset(SAMPLE).expect("parse");
        let module = lower(&unit).expect("lower");
        assert_eq!(module.uniforms.len(), 1);
        assert_eq!(module.uniforms[0].kind, SymbolKind::Uniform);
        assert_eq!(module.inputs[0].kind, SymbolKind::ShaderIn);
        assert_eq!(module.outputs[0].kind, SymbolKind::ShaderOut);
        assert!(module.uniforms[0].span.slice(SAMPLE).is_some());

        let trace = execute(&module, "main").expect("execute");
        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.steps[0].op, "load_const");
        assert_eq!(trace.steps[0].span.slice(SAMPLE), Some("float x = 1.0;"));
        assert_eq!(trace.steps[0].locals[0].value_bits, Some(1.0f32.to_bits()));
        assert_eq!(trace.steps[0].locals[1].value_bits, None);
        assert_eq!(trace.steps[1].op, "move");
        assert_eq!(trace.steps[1].span.slice(SAMPLE), Some("float y = x;"));
        assert_eq!(trace.steps[1].locals[1].value_bits, Some(1.0f32.to_bits()));
    }

    #[test]
    fn interface_reads_and_unassigned_locals_stay_outside_the_subset() {
        let read = "#version 300 es\nuniform float u_scale;\nvoid main() { float x = u_scale; }\n";
        let unit = parse_essl_subset(read).expect("parse uniform read");
        let error = lower(&unit).expect_err("uniform read");
        assert!(error.message.contains("interface variable"));

        let early = "#version 300 es\nvoid main() { float x = x; }\n";
        let unit = parse_essl_subset(early).expect("parse self read");
        let error = lower(&unit).expect_err("unassigned");
        assert!(error.message.contains("before it is assigned"));
    }

    #[test]
    fn compile_and_execute_reports_the_pipeline() {
        let trace = compile_and_execute(SAMPLE, "main").expect("pipeline");
        assert_eq!(trace.steps.len(), 2);
        let missing = compile_and_execute(SAMPLE, "missing").expect_err("missing function");
        assert!(missing.to_string().contains("unknown function"));
    }
}
