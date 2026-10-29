// Export the analyzed program currently open in Ghidra as a reve.engine.v1.Program.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.ClangLine;
import ghidra.app.decompiler.ClangToken;
import ghidra.app.decompiler.ClangTokenGroup;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.decompiler.DecompiledFunction;
import ghidra.app.decompiler.component.DecompilerUtils;
import ghidra.app.decompiler.parallel.ParallelDecompiler;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSetView;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.listing.Program;
import ghidra.program.model.lang.Language;
import ghidra.program.model.address.AddressSpace;
import ghidra.util.exception.CancelledException;
import com.google.protobuf.ByteString;
import generic.concurrent.QCallback;
import reve.engine.v1.Engine;

import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedDeque;

public class Export extends GhidraScript {
	private static final int DECOMPILATION_TIMEOUT_SECONDS = 30;

	@Override
	public void run() throws Exception {
		if (currentProgram == null) {
			throw new IllegalStateException("No program is open in Ghidra.");
		}

		String[] args = getScriptArgs();
		if (args.length != 1 || args[0].isBlank()) {
			throw new IllegalArgumentException("Usage: Export.java <output.pb>");
		}

		Path outputPath = Paths.get(args[0]).toAbsolutePath().normalize();
		Path parent = outputPath.getParent();
		if (parent != null) {
			Files.createDirectories(parent);
		}

		Engine.Program exported = collectProgram(currentProgram);
		try (OutputStream output = Files.newOutputStream(outputPath)) {
			exported.writeTo(output);
		}

		println("EXPORT_OK: " + outputPath + " (" + exported.getFunctionsCount() + " functions)");
	}

	private Engine.Program collectProgram(Program program) throws Exception {
		Language language = program.getLanguage();
		AddressSpace defaultSpace = language.getDefaultSpace();
		String format = program.getExecutableFormat();

		Engine.Program.Builder result = Engine.Program.newBuilder()
			.setName(program.getName())
			.setFormat(format == null ? "" : format)
			.setArchitecture(program.getLanguageID().toString())
			.setAddressSizeBits(program.getDefaultPointerSize() * Byte.SIZE);

		DecompInterface decompiler = new DecompInterface();
		decompiler.toggleCCode(true);
		String decompilerOpenError = openDecompiler(decompiler, program);
		Map<Address, Engine.Decompilation> decompilations = Map.of();
		if (decompilerOpenError == null) {
			decompilations = decompileFunctions(program, decompiler);
		} else {
			decompiler.dispose();
			println("WARNING: " + decompilerOpenError + "; function instructions will still be exported.");
		}

		Listing listing = program.getListing();
		FunctionIterator functions = program.getFunctionManager().getFunctions(true);
		while (functions.hasNext()) {
			monitor.checkCancelled();
			Function function = functions.next();
			Engine.Decompilation decompilation = decompilerOpenError == null
				? decompilations.get(function.getEntryPoint())
				: failedDecompilation(decompilerOpenError);
			result.addFunctions(collectFunction(function, listing, defaultSpace, decompilation));
		}
		return result.build();
	}

	private String openDecompiler(DecompInterface decompiler, Program program) {
		try {
			if (decompiler.openProgram(program)) {
				return null;
			}
			return diagnostic(decompiler.getLastMessage(), "Ghidra could not start the decompiler.");
		} catch (RuntimeException error) {
			return exceptionMessage(error);
		}
	}

	// Not DecompilerCallback: its pool opens each decompiler under the lock every release waits on.
	private Map<Address, Engine.Decompilation> decompileFunctions(
			Program program, DecompInterface opened) throws Exception {
		Deque<DecompInterface> idle = new ConcurrentLinkedDeque<>(List.of(opened));
		QCallback<Function, Map.Entry<Address, Engine.Decompilation>> callback = (function, itemMonitor) -> {
			Address entry = function.getEntryPoint();
			DecompInterface decompiler = idle.poll();
			if (decompiler == null) {
				decompiler = new DecompInterface();
				decompiler.toggleCCode(true);
				String openError = openDecompiler(decompiler, program);
				if (openError != null) {
					decompiler.dispose();
					return Map.entry(entry, failedDecompilation(openError));
				}
			}
			try {
				return Map.entry(entry, collectDecompilation(
					decompiler.decompileFunction(function, DECOMPILATION_TIMEOUT_SECONDS, itemMonitor)));
			} finally {
				idle.push(decompiler);
			}
		};

		Map<Address, Engine.Decompilation> decompilations = new ConcurrentHashMap<>();
		try {
			ParallelDecompiler.decompileFunctions(callback, program,
				program.getFunctionManager().getFunctions(true),
				entry -> decompilations.put(entry.getKey(), entry.getValue()), monitor);
		} finally {
			idle.forEach(DecompInterface::dispose);
		}
		monitor.checkCancelled();
		return decompilations;
	}

	private Engine.Function collectFunction(
			Function function, Listing listing, AddressSpace defaultSpace,
			Engine.Decompilation decompilation) throws Exception {
		AddressSetView body = function.getBody();
		long sizeInAddressUnits = body.getNumAddresses();
		long sizeInBytes = Math.multiplyExact(sizeInAddressUnits, defaultSpace.getAddressableUnitSize());

		Engine.Function.Builder result = Engine.Function.newBuilder()
			.setName(function.getName())
			.setEntry(function.getEntryPoint().getUnsignedOffset())
			.setSize(sizeInBytes);

		InstructionIterator instructions = listing.getInstructions(body, true);
		while (instructions.hasNext()) {
			monitor.checkCancelled();
			result.addInstructions(collectInstruction(instructions.next()));
		}
		result.setDecompilation(decompilation);
		return result.build();
	}

	private Engine.Decompilation collectDecompilation(DecompileResults results) throws Exception {
		try {
			if (results.isCancelled()) {
				if (monitor.isCancelled()) {
					monitor.checkCancelled();
				}
				return failedDecompilation("Ghidra cancelled this function's decompilation.");
			}
			if (results.isTimedOut()) {
				return timedOutDecompilation(diagnostic(
					results.getErrorMessage(), "Decompilation exceeded the "
						+ DECOMPILATION_TIMEOUT_SECONDS + " second time limit."));
			}
			if (!results.decompileCompleted()) {
				return failedDecompilation(diagnostic(
					results.getErrorMessage(), "Ghidra could not decompile this function."));
			}

			DecompiledFunction decompiled = results.getDecompiledFunction();
			if (decompiled == null) {
				return failedDecompilation("Ghidra completed decompilation without returning C code.");
			}

			Engine.Decompilation.Builder result = Engine.Decompilation.newBuilder()
				.setStatus(Engine.Decompilation.Status.SUCCEEDED)
				.setSignature(nullToEmpty(decompiled.getSignature()))
				.setCCode(nullToEmpty(decompiled.getC()));
			ClangTokenGroup markup = results.getCCodeMarkup();
			if (markup != null) {
				for (ClangLine line : DecompilerUtils.toLines(markup)) {
					result.addLines(collectLine(line));
				}
			}
			return result.build();
		} catch (Exception error) {
			if (error instanceof CancelledException || monitor.isCancelled()) {
				throw error;
			}
			return failedDecompilation(exceptionMessage(error));
		}
	}

	private Engine.Line collectLine(ClangLine line) {
		Engine.Line.Builder result = Engine.Line.newBuilder();
		String indent = line.getIndentString();
		if (!indent.isEmpty()) {
			result.addTokens(Engine.Token.newBuilder().setText(indent));
		}
		TreeSet<Long> addresses = new TreeSet<>(Long::compareUnsigned);
		for (ClangToken token : line.getAllTokens()) {
			String text = token.getText();
			if (text != null && !text.isEmpty()) {
				result.addTokens(Engine.Token.newBuilder()
					.setText(text)
					.setKind(tokenKind(token.getSyntaxType())));
			}
			Address address = token.getMinAddress();
			if (address != null) {
				addresses.add(address.getUnsignedOffset());
			}
		}
		return result.addAllAddresses(addresses).build();
	}

	private Engine.Token.Kind tokenKind(int syntaxType) {
		return switch (syntaxType) {
			case ClangToken.KEYWORD_COLOR -> Engine.Token.Kind.KEYWORD;
			case ClangToken.COMMENT_COLOR -> Engine.Token.Kind.COMMENT;
			case ClangToken.TYPE_COLOR -> Engine.Token.Kind.TYPE;
			case ClangToken.FUNCTION_COLOR -> Engine.Token.Kind.FUNCTION;
			case ClangToken.VARIABLE_COLOR -> Engine.Token.Kind.VARIABLE;
			case ClangToken.CONST_COLOR -> Engine.Token.Kind.CONSTANT;
			case ClangToken.PARAMETER_COLOR -> Engine.Token.Kind.PARAMETER;
			case ClangToken.GLOBAL_COLOR -> Engine.Token.Kind.GLOBAL;
			case ClangToken.ERROR_COLOR -> Engine.Token.Kind.ERROR;
			case ClangToken.SPECIAL_COLOR -> Engine.Token.Kind.SPECIAL;
			default -> Engine.Token.Kind.PLAIN;
		};
	}

	private Engine.Decompilation failedDecompilation(String message) {
		return Engine.Decompilation.newBuilder()
			.setStatus(Engine.Decompilation.Status.FAILED)
			.setErrorMessage(message)
			.build();
	}

	private Engine.Decompilation timedOutDecompilation(String message) {
		return Engine.Decompilation.newBuilder()
			.setStatus(Engine.Decompilation.Status.TIMED_OUT)
			.setErrorMessage(message)
			.build();
	}

	private String diagnostic(String message, String fallback) {
		return message == null || message.isBlank() ? fallback : message;
	}

	private String exceptionMessage(Exception error) {
		return diagnostic(error.getMessage(), error.getClass().getSimpleName());
	}

	private String nullToEmpty(String value) {
		return value == null ? "" : value;
	}

	private Engine.Instruction collectInstruction(Instruction instruction) throws Exception {
		StringBuilder operands = new StringBuilder();
		for (int index = 0; index < instruction.getNumOperands(); index++) {
			if (index > 0) {
				operands.append(", ");
			}
			operands.append(instruction.getDefaultOperandRepresentation(index));
		}

		return Engine.Instruction.newBuilder()
			.setAddress(instruction.getAddress().getUnsignedOffset())
			.setSize(instruction.getLength())
			.setMnemonic(instruction.getMnemonicString())
			.setOperands(operands.toString())
			.setBytes(ByteString.copyFrom(instruction.getBytes()))
			.build();
	}
}
