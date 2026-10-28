//! Self-authored bytecode and a CPU interpreter.
//!
//! Lowering is `ESSL AST -> bytecode`. Execution walks that bytecode and keeps
//! a source span, the local snapshot, and the uniform/input/output snapshot
//! after every instruction. There is no machine-code JIT. A single-step
//! debugger can later present `DebugStep` without changing the instruction format.

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

/// Where a new op reads a scalar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operand {
    Local(u16),
    Uniform(u16),
    Input(u16),
    Const(u32),
}

/// Where `mul` writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dest {
    Local(u16),
    Output(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Op {
    /// Write an f32 bit pattern into a local slot.
    LoadConst { dest: u16, value_bits: u32 },
    /// Copy one local slot into another.
    Move { dest: u16, src: u16 },
    /// Copy a uniform binding into a local slot.
    LoadUniform { dest: u16, index: u16 },
    /// Copy a vertex input into a local slot.
    LoadInput { dest: u16, index: u16 },
    /// `dest = lhs * rhs`. A non-finite product is a runtime error.
    Mul {
        dest: Dest,
        lhs: Operand,
        rhs: Operand,
    },
    /// Write a shader output. These outputs are the varyings for this subset.
    StoreOutput { index: u16, src: Operand },
}

impl Op {
    pub const fn name(self) -> &'static str {
        match self {
            Self::LoadConst { .. } => "load_const",
            Self::Move { .. } => "move",
            Self::LoadUniform { .. } => "load_uniform",
            Self::LoadInput { .. } => "load_input",
            Self::Mul { .. } => "mul",
            Self::StoreOutput { .. } => "store_output",
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

/// Uniform, vertex input, or vertex output after one instruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceSnapshot {
    pub name: String,
    pub kind: SymbolKind,
    pub value_bits: Option<u32>,
}

impl InterfaceSnapshot {
    pub fn value(&self) -> Option<f32> {
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
    /// Uniforms, inputs, then outputs, in declaration order.
    pub interfaces: Vec<InterfaceSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecTrace {
    pub steps: Vec<DebugStep>,
    pub outputs: Vec<InterfaceSnapshot>,
}

impl ExecTrace {
    pub fn output(&self, name: &str) -> Option<&InterfaceSnapshot> {
        self.outputs.iter().find(|item| item.name == name)
    }
}

struct LocalBuilder {
    name: String,
    span: SourceSpan,
    assigned: bool,
}

/// Lower every function in the subset AST.
///
/// Uniforms and vertex inputs can be read. Vertex outputs can be written.
/// Reading an output, or assigning a uniform or input, is outside this subset.
/// There is no `vec4` and no `gl_Position` yet.
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
                if symbol_index(uniforms, name).is_some() || symbol_index(inputs, name).is_some() {
                    return Err(lower_error(
                        format!("cannot assign interface variable `{name}` in this subset"),
                        *span,
                    ));
                }
                if let Some(index) = symbol_index(outputs, name) {
                    let op = lower_output(index, value, &locals, uniforms, inputs, outputs)?;
                    instructions.push(Instruction { op, span: *span });
                    continue;
                }
                let dest = local_slot(&locals, name)
                    .ok_or_else(|| lower_error(format!("unknown local `{name}`"), *span))?;
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
        Expr::Name { .. } => match lower_operand(value, locals, uniforms, inputs, outputs)? {
            Operand::Local(src) => Ok(Op::Move { dest, src }),
            Operand::Uniform(index) => Ok(Op::LoadUniform { dest, index }),
            Operand::Input(index) => Ok(Op::LoadInput { dest, index }),
            Operand::Const(bits) => Ok(Op::LoadConst {
                dest,
                value_bits: bits,
            }),
        },
        Expr::Mul { lhs, rhs, .. } => Ok(Op::Mul {
            dest: Dest::Local(dest),
            lhs: lower_operand(lhs, locals, uniforms, inputs, outputs)?,
            rhs: lower_operand(rhs, locals, uniforms, inputs, outputs)?,
        }),
    }
}

fn lower_output(
    index: u16,
    value: &Expr,
    locals: &[LocalBuilder],
    uniforms: &[DebugSymbol],
    inputs: &[DebugSymbol],
    outputs: &[DebugSymbol],
) -> Result<Op, LowerError> {
    match value {
        Expr::Mul { lhs, rhs, .. } => Ok(Op::Mul {
            dest: Dest::Output(index),
            lhs: lower_operand(lhs, locals, uniforms, inputs, outputs)?,
            rhs: lower_operand(rhs, locals, uniforms, inputs, outputs)?,
        }),
        _ => Ok(Op::StoreOutput {
            index,
            src: lower_operand(value, locals, uniforms, inputs, outputs)?,
        }),
    }
}

fn lower_operand(
    value: &Expr,
    locals: &[LocalBuilder],
    uniforms: &[DebugSymbol],
    inputs: &[DebugSymbol],
    outputs: &[DebugSymbol],
) -> Result<Operand, LowerError> {
    match value {
        Expr::Float { bits, .. } => Ok(Operand::Const(*bits)),
        Expr::Name { name, span } => resolve_name(name, *span, locals, uniforms, inputs, outputs),
        Expr::Mul { span, .. } => Err(lower_error(
            "nested multiplication is outside this subset",
            *span,
        )),
    }
}

fn resolve_name(
    name: &str,
    span: SourceSpan,
    locals: &[LocalBuilder],
    uniforms: &[DebugSymbol],
    inputs: &[DebugSymbol],
    outputs: &[DebugSymbol],
) -> Result<Operand, LowerError> {
    if let Some(src) = local_slot(locals, name) {
        if !locals[src as usize].assigned {
            return Err(lower_error(
                format!("local `{name}` is read before it is assigned"),
                span,
            ));
        }
        return Ok(Operand::Local(src));
    }
    if let Some(index) = symbol_index(uniforms, name) {
        return Ok(Operand::Uniform(index));
    }
    if let Some(index) = symbol_index(inputs, name) {
        return Ok(Operand::Input(index));
    }
    if symbol_index(outputs, name).is_some() {
        return Err(lower_error(
            format!("reading shader output `{name}` is outside this subset"),
            span,
        ));
    }
    Err(lower_error(format!("unknown local `{name}`"), span))
}

fn symbol_index(symbols: &[DebugSymbol], name: &str) -> Option<u16> {
    symbols
        .iter()
        .position(|symbol| symbol.name == name)
        .and_then(|index| u16::try_from(index).ok())
}

fn local_slot(locals: &[LocalBuilder], name: &str) -> Option<u16> {
    locals
        .iter()
        .position(|local| local.name == name)
        .map(|index| index as u16)
}

fn lower_error(message: impl Into<String>, span: SourceSpan) -> LowerError {
    LowerError {
        message: message.into(),
        span,
    }
}

/// Interpret one lowered function on the CPU, with no uniform or input bindings.
///
/// Locals start unassigned. A load of an unbound uniform or input is an error.
/// Each step records the source span, locals, and interface values.
pub fn execute(module: &BytecodeModule, function_name: &str) -> Result<ExecTrace, VmError> {
    execute_bound(module, function_name, &[], &[])
}

/// Interpret one function with uniform and per-vertex input bindings.
///
/// Bindings are scalar floats addressed by name. Unknown and duplicate names
/// are errors. Outputs start unassigned and are the varyings of this subset.
pub fn execute_bound(
    module: &BytecodeModule,
    function_name: &str,
    uniforms: &[(&str, f32)],
    inputs: &[(&str, f32)],
) -> Result<ExecTrace, VmError> {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == function_name)
        .ok_or_else(|| VmError {
            message: format!("unknown function `{function_name}`"),
        })?;
    let uniform_values = bind_symbols(&module.uniforms, uniforms, "uniform")?;
    let input_values = bind_symbols(&module.inputs, inputs, "input")?;
    let mut output_values = vec![None; module.outputs.len()];
    let mut values = vec![None; function.locals.len()];
    let mut steps = Vec::with_capacity(function.instructions.len());
    for (pc, instruction) in function.instructions.iter().enumerate() {
        match instruction.op {
            Op::LoadConst { dest, value_bits } => {
                write_local(dest, value_bits, &mut values)?;
            }
            Op::Move { dest, src } => {
                let value = read_local(src, &values)?;
                write_local(dest, value, &mut values)?;
            }
            Op::LoadUniform { dest, index } => {
                let value = bound_value(&uniform_values, index, &module.uniforms, "uniform")?;
                write_local(dest, value, &mut values)?;
            }
            Op::LoadInput { dest, index } => {
                let value = bound_value(&input_values, index, &module.inputs, "input")?;
                write_local(dest, value, &mut values)?;
            }
            Op::Mul { dest, lhs, rhs } => {
                let left = f32::from_bits(read_operand(
                    lhs,
                    &values,
                    &uniform_values,
                    &input_values,
                    module,
                )?);
                let right = f32::from_bits(read_operand(
                    rhs,
                    &values,
                    &uniform_values,
                    &input_values,
                    module,
                )?);
                let product = left * right;
                if !product.is_finite() {
                    return Err(VmError {
                        message: "multiplication result is not finite".to_string(),
                    });
                }
                write_dest(dest, product.to_bits(), &mut values, &mut output_values)?;
            }
            Op::StoreOutput { index, src } => {
                let value = read_operand(src, &values, &uniform_values, &input_values, module)?;
                write_output(index, value, &mut output_values)?;
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
            interfaces: interface_snapshot(module, &uniform_values, &input_values, &output_values),
        });
    }
    Ok(ExecTrace {
        steps,
        outputs: interface_rows(&module.outputs, &output_values),
    })
}

fn bind_symbols(
    symbols: &[DebugSymbol],
    bindings: &[(&str, f32)],
    kind: &str,
) -> Result<Vec<Option<u32>>, VmError> {
    let mut values = vec![None; symbols.len()];
    let mut seen = vec![false; symbols.len()];
    for (name, value) in bindings {
        if !value.is_finite() {
            return Err(VmError {
                message: format!("{kind} `{name}` is not finite"),
            });
        }
        let Some(index) = symbols.iter().position(|symbol| symbol.name == *name) else {
            return Err(VmError {
                message: format!("unknown {kind} `{name}`"),
            });
        };
        if seen[index] {
            return Err(VmError {
                message: format!("duplicate {kind} `{name}`"),
            });
        }
        seen[index] = true;
        values[index] = Some(value.to_bits());
    }
    Ok(values)
}

fn read_operand(
    operand: Operand,
    locals: &[Option<u32>],
    uniforms: &[Option<u32>],
    inputs: &[Option<u32>],
    module: &BytecodeModule,
) -> Result<u32, VmError> {
    match operand {
        Operand::Const(bits) => Ok(bits),
        Operand::Local(index) => read_local(index, locals),
        Operand::Uniform(index) => bound_value(uniforms, index, &module.uniforms, "uniform"),
        Operand::Input(index) => bound_value(inputs, index, &module.inputs, "input"),
    }
}

fn read_local(index: u16, locals: &[Option<u32>]) -> Result<u32, VmError> {
    locals
        .get(index as usize)
        .copied()
        .flatten()
        .ok_or_else(|| VmError {
            message: format!("local slot {index} is unassigned"),
        })
}

fn bound_value(
    values: &[Option<u32>],
    index: u16,
    symbols: &[DebugSymbol],
    kind: &str,
) -> Result<u32, VmError> {
    match values.get(index as usize) {
        Some(Some(bits)) => Ok(*bits),
        Some(None) => {
            let name = symbols
                .get(index as usize)
                .map(|symbol| symbol.name.as_str())
                .unwrap_or("?");
            Err(VmError {
                message: format!("{kind} `{name}` has no binding"),
            })
        }
        None => Err(VmError {
            message: format!("{kind} slot {index} is missing"),
        }),
    }
}

fn write_local(index: u16, bits: u32, locals: &mut [Option<u32>]) -> Result<(), VmError> {
    let slot = locals.get_mut(index as usize).ok_or_else(|| VmError {
        message: format!("local slot {index} is missing"),
    })?;
    *slot = Some(bits);
    Ok(())
}

fn write_output(index: u16, bits: u32, outputs: &mut [Option<u32>]) -> Result<(), VmError> {
    let slot = outputs.get_mut(index as usize).ok_or_else(|| VmError {
        message: format!("output slot {index} is missing"),
    })?;
    *slot = Some(bits);
    Ok(())
}

fn write_dest(
    dest: Dest,
    bits: u32,
    locals: &mut [Option<u32>],
    outputs: &mut [Option<u32>],
) -> Result<(), VmError> {
    match dest {
        Dest::Local(index) => write_local(index, bits, locals),
        Dest::Output(index) => write_output(index, bits, outputs),
    }
}

fn interface_snapshot(
    module: &BytecodeModule,
    uniforms: &[Option<u32>],
    inputs: &[Option<u32>],
    outputs: &[Option<u32>],
) -> Vec<InterfaceSnapshot> {
    let mut rows = interface_rows(&module.uniforms, uniforms);
    rows.extend(interface_rows(&module.inputs, inputs));
    rows.extend(interface_rows(&module.outputs, outputs));
    rows
}

fn interface_rows(symbols: &[DebugSymbol], values: &[Option<u32>]) -> Vec<InterfaceSnapshot> {
    symbols
        .iter()
        .zip(values.iter())
        .map(|(symbol, value)| InterfaceSnapshot {
            name: symbol.name.clone(),
            kind: symbol.kind,
            value_bits: *value,
        })
        .collect()
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
        assert_eq!(trace.steps[0].interfaces.len(), 3);
        assert_eq!(trace.steps[0].interfaces[0].name, "u_scale");
        assert_eq!(trace.steps[0].interfaces[0].kind, SymbolKind::Uniform);
        assert_eq!(trace.steps[0].interfaces[0].value_bits, None);
        assert_eq!(trace.output("v_out").expect("output").value_bits, None);
    }

    #[test]
    fn uniform_input_and_output_drive_one_invocation() {
        let source = "#version 300 es\nuniform float u_scale;\nin float a_x;\nout float v_clip_x;\nvoid main() {\n  float x = a_x * u_scale;\n  v_clip_x = x;\n}\n";
        let unit = parse_essl_subset(source).expect("parse");
        let module = lower(&unit).expect("lower");
        assert_eq!(
            module.functions[0].instructions[0].op,
            Op::Mul {
                dest: Dest::Local(0),
                lhs: Operand::Input(0),
                rhs: Operand::Uniform(0),
            }
        );
        assert_eq!(
            module.functions[0].instructions[1].op,
            Op::StoreOutput {
                index: 0,
                src: Operand::Local(0),
            }
        );

        let missing = execute(&module, "main").expect_err("unbound");
        assert!(missing.to_string().contains("has no binding"));
        let missing = execute_bound(&module, "main", &[], &[("a_x", 0.75)]).expect_err("uniform");
        assert!(missing.to_string().contains("u_scale"));

        let trace =
            execute_bound(&module, "main", &[("u_scale", 2.0)], &[("a_x", 0.75)]).expect("bound");
        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.steps[0].op, "mul");
        assert_eq!(
            trace.steps[0].span.slice(source),
            Some("float x = a_x * u_scale;")
        );
        assert_eq!(trace.steps[0].locals[0].value_bits, Some(1.5f32.to_bits()));
        assert_eq!(
            trace.steps[0].interfaces[0].value_bits,
            Some(2.0f32.to_bits())
        );
        assert_eq!(trace.steps[1].op, "store_output");
        assert_eq!(trace.steps[1].locals[0].value_bits, Some(1.5f32.to_bits()));
        assert_eq!(
            trace.output("v_clip_x").expect("varying").value_bits,
            Some(1.5f32.to_bits())
        );
    }

    #[test]
    fn output_reads_and_unassigned_locals_stay_outside_the_subset() {
        let read = "#version 300 es\nout float v_out;\nvoid main() { float x = v_out; }\n";
        let unit = parse_essl_subset(read).expect("parse output read");
        let error = lower(&unit).expect_err("output read");
        assert!(error.message.contains("shader output"));

        let assign = "#version 300 es\nuniform float u_scale;\nvoid main() { u_scale = 1.0; }\n";
        let unit = parse_essl_subset(assign).expect("parse assign");
        let error = lower(&unit).expect_err("uniform assign");
        assert!(error.message.contains("cannot assign"));

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
