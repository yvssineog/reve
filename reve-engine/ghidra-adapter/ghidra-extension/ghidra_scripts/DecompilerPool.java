import java.util.ArrayDeque;
import java.util.Deque;
import java.util.concurrent.Callable;
import java.util.function.Consumer;

/**
 * Lends reusable decompilers to parallel workers. {@link #close} is the only place that disposes
 * them, and it waits for every worker first, so each decompiler is disposed exactly once.
 */
final class DecompilerPool<D> {
	interface Task<D, R> {
		R run(D decompiler) throws Exception;
	}

	private final Consumer<D> dispose;
	private final Deque<D> idle = new ArrayDeque<>();
	private int running;
	private boolean closed;

	DecompilerPool(D opened, Consumer<D> dispose) {
		this.dispose = dispose;
		idle.push(opened);
	}

	/** Runs {@code task} with an idle decompiler, opening one when none is idle. */
	<R> R run(Callable<D> open, Task<D, R> task) throws Exception {
		D decompiler;
		synchronized (this) {
			if (closed) {
				throw new IllegalStateException("The decompiler pool is closed.");
			}
			running++;
			decompiler = idle.poll();
		}
		try {
			if (decompiler == null) {
				decompiler = open.call();
			}
			return task.run(decompiler);
		} finally {
			finish(decompiler);
		}
	}

	private synchronized void finish(D decompiler) {
		if (decompiler != null) {
			idle.push(decompiler);
		}
		running--;
		notifyAll();
	}

	/**
	 * Refuses new work, waits for running workers, then disposes every decompiler. Interrupts
	 * don't end the wait, because Ghidra closes the program once the script returns and workers
	 * must not outlive it. The decompiler's own timeout bounds each worker.
	 */
	void close() {
		boolean interrupted = false;
		synchronized (this) {
			closed = true;
			while (running > 0) {
				try {
					wait();
				} catch (InterruptedException error) {
					interrupted = true;
				}
			}
		}
		for (D decompiler; (decompiler = idle.poll()) != null; ) {
			dispose.accept(decompiler);
		}
		if (interrupted) {
			Thread.currentThread().interrupt();
		}
	}
}
