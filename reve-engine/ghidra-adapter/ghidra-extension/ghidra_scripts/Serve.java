// Keep the program open and make rêve's edits to it, one at a time, over the connection rêve
// listens on. Usage: Serve.java <port> <token>
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Bookmark;
import ghidra.program.model.listing.BookmarkManager;
import ghidra.program.model.listing.BookmarkType;
import ghidra.program.model.listing.CommentType;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.SourceType;
import ghidra.util.exception.InvalidInputException;
import reve.engine.v1.Engine;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

public class Serve extends Export {
	@Override
	public void run() throws Exception {
		String[] args = getScriptArgs();
		if (args.length != 2) {
			throw new IllegalArgumentException("Usage: Serve.java <port> <token>");
		}
		// Headless runs a script in one transaction, which keeps the program from saving until the
		// script ends. Each edit takes its own transaction and saves instead.
		end(true);
		try (Socket socket = new Socket(InetAddress.getLoopbackAddress(), Integer.parseInt(args[0]))) {
			socket.setTcpNoDelay(true);
			InputStream in = new BufferedInputStream(socket.getInputStream());
			OutputStream out = new BufferedOutputStream(socket.getOutputStream());
			out.write((args[1] + "\n").getBytes(StandardCharsets.UTF_8));
			out.flush();
			// rêve closing the connection ends the script, and Ghidra exits.
			for (Engine.Edit edit; (edit = Engine.Edit.parseDelimitedFrom(in)) != null; ) {
				apply(edit).writeDelimitedTo(out);
				out.flush();
			}
		} finally {
			start();
		}
	}

	// Makes `edit` and saves the program, then decompiles again the functions whose C it changed.
	// An edit Ghidra refuses changes nothing and says why.
	private Engine.Changes apply(Engine.Edit edit) throws Exception {
		Engine.Changes.Builder changes = Engine.Changes.newBuilder();
		Set<Function> named = Set.of();
		int transaction = currentProgram.startTransaction("rêve " + edit.getKindCase());
		boolean commit = false;
		try {
			switch (edit.getKindCase()) {
				case RENAME -> named = rename(edit.getRename());
				case COMMENT -> changes.addComments(comment(edit.getComment()));
				case BOOKMARK -> changes.addBookmarks(bookmark(edit.getBookmark()));
				default -> throw new InvalidInputException("This rêve extension can't make that edit.");
			}
			commit = true;
		} catch (Exception error) {
			return Engine.Changes.newBuilder().setError(exceptionMessage(error)).build();
		} finally {
			currentProgram.endTransaction(transaction, commit);
		}
		currentProgram.save("rêve " + edit.getKindCase(), monitor);

		// ponytail: opens decompilers for every rename; keep a warm pool if renames feel slow.
		Map<Address, Engine.Decompilation> decompiled = decompile(currentProgram, named.iterator());
		for (Function function : named) {
			changes.addFunctions(Engine.Function.newBuilder()
				.setName(function.getName())
				.setEntry(function.getEntryPoint().getUnsignedOffset())
				.setDecompilation(decompiled.get(function.getEntryPoint())));
		}
		return changes.build();
	}

	// Names the function, or gives it Ghidra's default name back when the name is empty. Returns the
	// functions whose C shows the name: the function, its thunks, and those referring to either.
	private Set<Function> rename(Engine.Rename rename) throws Exception {
		Function function = getFunctionAt(toAddr(rename.getEntry()));
		if (function == null) {
			throw new InvalidInputException("No function starts at " + toAddr(rename.getEntry()) + ".");
		}
		if (rename.getName().isEmpty()) {
			function.setName(null, SourceType.DEFAULT);
		} else {
			function.setName(rename.getName(), SourceType.USER_DEFINED);
		}

		Set<Function> named = new LinkedHashSet<>();
		named.add(function);
		Address[] thunks = function.getFunctionThunkAddresses(true);
		if (thunks != null) {
			for (Address thunk : thunks) {
				Function thunkFunction = getFunctionAt(thunk);
				if (thunkFunction != null) {
					named.add(thunkFunction);
				}
			}
		}
		for (Function target : List.copyOf(named)) {
			for (Reference reference : getReferencesTo(target.getEntryPoint())) {
				Function referrer = getFunctionContaining(reference.getFromAddress());
				if (referrer != null) {
					named.add(referrer);
				}
			}
		}
		return named;
	}

	// Sets the instruction's end-of-line comment, or removes it when the text is empty.
	private Engine.Comment comment(Engine.Comment comment) throws InvalidInputException {
		Address address = toAddr(comment.getAddress());
		Instruction instruction = getInstructionAt(address);
		if (instruction == null) {
			throw new InvalidInputException("No instruction at " + address + ".");
		}
		instruction.setComment(CommentType.EOL, comment.getText().isEmpty() ? null : comment.getText());
		String now = instruction.getComment(CommentType.EOL);
		return comment.toBuilder().setText(now == null ? "" : now).build();
	}

	// Adds a Note bookmark at the address, or removes every Note bookmark there.
	private Engine.Bookmark bookmark(Engine.Bookmark bookmark) {
		Address address = toAddr(bookmark.getAddress());
		BookmarkManager bookmarks = currentProgram.getBookmarkManager();
		Bookmark[] notes = bookmarks.getBookmarks(address, BookmarkType.NOTE);
		if (bookmark.getOn() && notes.length == 0) {
			bookmarks.setBookmark(address, BookmarkType.NOTE, "", "");
		} else if (!bookmark.getOn()) {
			for (Bookmark note : notes) {
				bookmarks.removeBookmark(note);
			}
		}
		return bookmark;
	}
}
