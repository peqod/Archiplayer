export type LiveRequest =
  | { action: "switch"; generation: number }
  | { action: "toggle"; generation: number }
  | { action: "coalesce"; generation: number };

/** Small synchronous gate around the player's async media/API transitions. */
export class PlaybackTransitions {
  private generation = 0;
  private source: string | null = null;
  private connecting = false;

  requestLive(source: string): LiveRequest {
    if (this.source === source) {
      return {
        action: this.connecting ? "coalesce" : "toggle",
        generation: this.generation,
      };
    }
    return { action: "switch", generation: this.start(source) };
  }

  start(source: string): number {
    this.generation += 1;
    this.source = source;
    this.connecting = true;
    return this.generation;
  }

  settle(generation: number): void {
    if (this.isCurrent(generation)) this.connecting = false;
  }

  reset(): number {
    this.generation += 1;
    this.source = null;
    this.connecting = false;
    return this.generation;
  }

  isCurrent(generation: number): boolean {
    return generation === this.generation;
  }
}

export function isAbortError(error: unknown): boolean {
  return (
    (error instanceof DOMException && error.name === "AbortError") ||
    (typeof error === "object" &&
      error !== null &&
      "name" in error &&
      (error as { name?: unknown }).name === "AbortError")
  );
}

