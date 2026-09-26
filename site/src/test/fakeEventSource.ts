/**
 * A stand-in for the browser's `EventSource` (jsdom has none). Install it with
 * `vi.stubGlobal('EventSource', FakeEventSource)`; every construction is
 * recorded in `instances`, and `emit(type)` calls the listeners registered
 * for `type` as the browser would for a server-sent event of that name.
 */
export class FakeEventSource {
  static instances: FakeEventSource[] = [];

  readonly url: string;
  closed = false;
  onerror: ((event: Event) => void) | null = null;
  private listeners = new Map<string, Set<(event: MessageEvent) => void>>();

  constructor(url: string | URL) {
    this.url = String(url);
    FakeEventSource.instances.push(this);
  }

  /** The newest instance, the one a test usually drives. */
  static get latest(): FakeEventSource {
    const latest = FakeEventSource.instances.at(-1);
    if (!latest) throw new Error('no EventSource was opened');
    return latest;
  }

  static reset(): void {
    FakeEventSource.instances = [];
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void): void {
    const set = this.listeners.get(type) ?? new Set();
    set.add(listener);
    this.listeners.set(type, set);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void): void {
    this.listeners.get(type)?.delete(listener);
  }

  /** The event names with at least one listener. */
  get listenedTo(): string[] {
    return [...this.listeners].filter(([, set]) => set.size > 0).map(([type]) => type);
  }

  close(): void {
    this.closed = true;
  }

  emit(type: string, data = '{"id":"x"}'): void {
    if (this.closed) return;
    const event = new MessageEvent(type, { data });
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
}
