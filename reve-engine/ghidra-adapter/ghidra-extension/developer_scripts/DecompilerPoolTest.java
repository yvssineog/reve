import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

public class DecompilerPoolTest {
	private static final long TIMEOUT_MILLIS = 10_000;

	public static void main(String[] args) throws Exception {
		Map<String, Integer> disposals = new ConcurrentHashMap<>();
		DecompilerPool<String> pool =
			new DecompilerPool<>("seed", name -> disposals.merge(name, 1, Integer::sum));

		CountDownLatch paused = new CountDownLatch(1);
		CountDownLatch resume = new CountDownLatch(1);
		Thread pausedWorker = new Thread(() -> {
			try {
				pool.run(() -> "unused", decompiler -> {
					paused.countDown();
					resume.await();
					return decompiler;
				});
			} catch (Exception error) {
				throw new AssertionError(error);
			}
		});
		pausedWorker.setDaemon(true);
		pausedWorker.start();
		paused.await();

		RuntimeException thrown = new RuntimeException("injected");
		check(failure(() -> pool.run(() -> "opened", decompiler -> {
			throw thrown;
		})) == thrown, "a task's exception reaches its caller");

		RuntimeException openFailed = new RuntimeException("open failed");
		String reused = pool.run(() -> {
			throw openFailed;
		}, decompiler -> decompiler);
		check(reused.equals("opened"), "a thrown task returns its decompiler for reuse");

		pool.run(() -> "unused", decompiler -> {
			check(failure(() -> pool.run(() -> {
				throw openFailed;
			}, nested -> nested)) == openFailed, "a failed open reaches its caller");
			return decompiler;
		});

		AtomicBoolean interruptedAfterClose = new AtomicBoolean();
		Thread closer = new Thread(() -> {
			pool.close();
			interruptedAfterClose.set(Thread.currentThread().isInterrupted());
		});
		closer.setDaemon(true);
		closer.start();
		awaitWaiting(closer);
		check(disposals.isEmpty(), "nothing is disposed while a worker runs");

		AtomicBoolean opened = new AtomicBoolean();
		check(failure(() -> pool.run(() -> {
			opened.set(true);
			return "late";
		}, decompiler -> decompiler)) instanceof IllegalStateException, "a closing pool refuses work");
		check(!opened.get(), "a closing pool opens nothing");

		closer.interrupt();
		awaitWaiting(closer);
		check(disposals.isEmpty(), "an interrupt doesn't end the wait for workers");

		resume.countDown();
		pausedWorker.join(TIMEOUT_MILLIS);
		closer.join(TIMEOUT_MILLIS);
		check(!closer.isAlive(), "close returns once workers finish");
		check(interruptedAfterClose.get(), "close restores the interrupt");
		check(disposals.equals(Map.of("seed", 1, "opened", 1)), "each is disposed once: " + disposals);

		System.out.println("DECOMPILER_POOL_OK");
	}

	private interface Action {
		void run() throws Exception;
	}

	private static Exception failure(Action action) {
		AtomicReference<Exception> caught = new AtomicReference<>();
		try {
			action.run();
		} catch (Exception error) {
			caught.set(error);
		}
		return caught.get();
	}

	private static void awaitWaiting(Thread thread) throws InterruptedException {
		// A loop, not one poll: the interrupted thread is briefly RUNNABLE before it waits again.
		long deadline = System.currentTimeMillis() + TIMEOUT_MILLIS;
		Thread.sleep(50);
		while (thread.getState() != Thread.State.WAITING) {
			check(System.currentTimeMillis() < deadline, "close waits for the running worker");
			Thread.sleep(10);
		}
	}

	private static void check(boolean condition, String message) {
		if (!condition) {
			throw new AssertionError(message);
		}
	}
}
