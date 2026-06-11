type AsyncFn<Args extends unknown[], R> = (...args: Args) => Promise<R>

// Returns function that ensures no parallel execution of fn and coalesces multiple calls
// while execution is active into a single follow-up call
export function coalesceTrailing<Args extends unknown[], R>(
  fn: AsyncFn<Args, R>,
): AsyncFn<Args, R> {
  let running = false
  let pending = false
  let pendingArgs: Args | undefined
  let pendingResolvers: Array<{
    resolve: (value: R) => void
    reject: (reason?: unknown) => void
  }> = []

  async function run(args: Args, resolvers: typeof pendingResolvers): Promise<void> {
    try {
      const value = await fn(...args)
      resolvers.forEach(({ resolve }) => resolve(value))
    } catch (err) {
      resolvers.forEach(({ reject }) => reject(err))
    }
  }

  return function wrapped(...args: Args): Promise<R> {
    if (!running) {
      running = true

      return new Promise<R>((resolve, reject) => {
        void run(args, [{ resolve, reject }]).finally(async () => {
          while (pending && pendingArgs) {
            const argsToRun = pendingArgs
            const resolvers = pendingResolvers

            pending = false
            pendingArgs = undefined
            pendingResolvers = []

            await run(argsToRun, resolvers)
          }

          running = false
        })
      })
    }

    pending = true
    pendingArgs = args

    return new Promise<R>((resolve, reject) => {
      pendingResolvers.push({ resolve, reject })
    })
  }
}

export function delay(ms: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, ms))
}

export function assertNever(value: never): never {
  throw new Error(`Unexpected code path reached due to value: ${value}`)
}
