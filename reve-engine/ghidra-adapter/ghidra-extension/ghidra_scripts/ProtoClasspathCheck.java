// Quick Ghidra-side check that the generated protobuf types and runtime are
// available to scripts loaded from this extension.
import ghidra.app.script.GhidraScript;
import reve.engine.v1.Engine;

public class ProtoClasspathCheck extends GhidraScript {
	@Override
	public void run() throws Exception {
		Engine.Program message = Engine.Program.newBuilder()
			.setName(currentProgram.getName())
			.setFormat("classpath-check")
			.build();

		Engine.Program decoded = Engine.Program.parseFrom(message.toByteArray());
		if (!decoded.getName().equals(currentProgram.getName())) {
			throw new IllegalStateException("protobuf serialization round-trip failed");
		}

		println("PROTO_CLASSPATH_OK: " + decoded.getName());
	}
}
