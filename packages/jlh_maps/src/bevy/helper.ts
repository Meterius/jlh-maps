export class TickGate {
  private holderId = 0
  private holders = new Map<number, TickGateHolder>()

  private awaitedTicks = new Map<
    number,
    {
      promise: Promise<TickGateWaitResult>
      resolve: (result: TickGateWaitResult) => void
    }
  >()

  registerHolder(): TickGateHolder {
    const holderId = this.holderId++
    const holder = new TickGateHolder(this, holderId)
    this.holders.set(holderId, holder)
    this.notify()
    return holder
  }

  unregisterHolder(gateHolder: TickGateHolder): void {
    this.holders.delete(gateHolder.id)
    this.notify()
  }

  notify() {
    this.awaitedTicks.forEach(({ resolve }, tick) => {
      if (this.isTickReleased(tick)) {
        resolve({
          released: true,
          pendingHolderIds: [],
        })
        this.awaitedTicks.delete(tick)
      }
    })
  }

  untilTickReleased(tick: number, timeoutMs: number): Promise<TickGateWaitResult> {
    const prev = this.awaitedTicks.get(tick)
    if (prev) return prev.promise

    if (this.isTickReleased(tick)) {
      return Promise.resolve({
        released: true,
        pendingHolderIds: [],
      })
    }

    let timeout: ReturnType<typeof setTimeout> | undefined
    let finish: (result: TickGateWaitResult) => void = () => {}
    const promise = new Promise<TickGateWaitResult>((resolve) => {
      finish = (result: TickGateWaitResult) => {
        if (timeout !== undefined) {
          clearTimeout(timeout)
          timeout = undefined
        }
        resolve(result)
      }
    })

    this.awaitedTicks.set(tick, { promise, resolve: finish })

    timeout = setTimeout(() => {
      this.awaitedTicks.delete(tick)
      finish({
        released: false,
        pendingHolderIds: this.pendingHolderIds(tick),
      })
    }, timeoutMs)

    return promise
  }

  minReleasedTick(): number {
    if (this.holders.size === 0) return Number.POSITIVE_INFINITY

    return Math.min(...[...this.holders.values()].map((holder) => holder.getReleasedTick()))
  }

  isTickReleased(tick: number) {
    return this.minReleasedTick() >= tick
  }

  pendingHolderIds(tick: number): number[] {
    return [...this.holders.values()]
      .filter((holder) => holder.getReleasedTick() < tick)
      .map((holder) => holder.id)
  }

  free() {
    this.holders.clear()
    this.notify()
  }
}

export class TickGateHolder {
  private releasedTick = 0
  private released = false

  constructor(
    private readonly gate: TickGate,
    readonly id: number,
  ) {}

  release(tick: number): void {
    if (this.released) return

    this.releasedTick = Math.max(this.releasedTick, tick)
    this.gate.notify()
  }

  free(): void {
    if (this.released) return

    this.released = true
    this.gate.unregisterHolder(this)
  }

  getReleasedTick(): number {
    return this.releasedTick
  }
}

export interface TickGateWaitResult {
  released: boolean
  pendingHolderIds: number[]
}
