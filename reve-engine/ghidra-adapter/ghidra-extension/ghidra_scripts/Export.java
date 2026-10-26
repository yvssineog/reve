// Export the analyzed program currently open in Ghidra as a reve.engine.v1.Program.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.AddressSetView;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.listing.Program;
import ghidra.program.model.lang.Language;
import ghidra.program.model.address.AddressSpace;
import com.google.protobuf.ByteString;
import reve.engine.v1.Engine;

import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

public class Export extends GhidraScript {
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

		Listing listing = program.getListing();
		FunctionIterator functions = program.getFunctionManager().getFunctions(true);
		while (functions.hasNext()) {
			monitor.checkCancelled();
			result.addFunctions(collectFunction(functions.next(), listing, defaultSpace));
		}
		return result.build();
	}

	private Engine.Function collectFunction(
			Function function, Listing listing, AddressSpace defaultSpace) throws Exception {
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
		return result.build();
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
