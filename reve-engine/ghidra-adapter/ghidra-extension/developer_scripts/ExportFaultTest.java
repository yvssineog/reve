import ghidra.app.decompiler.DecompInterface;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.listing.Listing;
import ghidra.util.task.TaskMonitor;
import java.nio.file.Files;
import java.nio.file.Paths;
import reve.engine.v1.Engine;

/** Exports with one decompilation throwing, then checks everything else still made it out. */
public class ExportFaultTest extends Export {
	private static final String INJECTED = "injected decompilation failure";

	private Address faulty;

	@Override
	protected Engine.Decompilation collectDecompilation(
			Function function, DecompInterface decompiler, TaskMonitor itemMonitor) throws Exception {
		if (function.getEntryPoint().equals(faulty)) {
			throw new IllegalStateException(INJECTED);
		}
		return super.collectDecompilation(function, decompiler, itemMonitor);
	}

	@Override
	public void run() throws Exception {
		faulty = currentProgram.getFunctionManager().getFunctions(true).next().getEntryPoint();
		super.run();

		Engine.Program exported =
			Engine.Program.parseFrom(Files.readAllBytes(Paths.get(getScriptArgs()[0])));
		Listing listing = currentProgram.getListing();
		int succeeded = 0;
		int expectedFunctions = 0;
		for (Function function : currentProgram.getFunctionManager().getFunctions(true)) {
			Engine.Function out = exported.getFunctions(expectedFunctions++);
			Engine.Decompilation decompilation = out.getDecompilation();
			boolean isFaulty = function.getEntryPoint().equals(faulty);
			if (isFaulty != INJECTED.equals(decompilation.getErrorMessage())) {
				throw new AssertionError(function.getName() + ": " + decompilation.getErrorMessage());
			}
			if (isFaulty && decompilation.getStatus() != Engine.Decompilation.Status.FAILED) {
				throw new AssertionError("the throwing function isn't FAILED: " + decompilation.getStatus());
			}
			if (decompilation.getStatus() == Engine.Decompilation.Status.SUCCEEDED) {
				succeeded++;
			}
			int instructions = 0;
			InstructionIterator iterator = listing.getInstructions(function.getBody(), true);
			while (iterator.hasNext()) {
				iterator.next();
				instructions++;
			}
			if (out.getInstructionsCount() != instructions) {
				throw new AssertionError(function.getName() + " lost instructions: "
					+ out.getInstructionsCount() + " of " + instructions);
			}
		}
		if (exported.getFunctionsCount() != expectedFunctions || succeeded == 0) {
			throw new AssertionError("exported " + exported.getFunctionsCount() + " of "
				+ expectedFunctions + ", " + succeeded + " succeeded");
		}
		println("EXPORT_FAULT_OK: " + expectedFunctions + " functions, " + succeeded + " succeeded");
	}
}
