/* tslint:disable */
/* eslint-disable */

export class WebGame {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Interleaved stereo PCM, f32 in [-1, 1]: L0, R0, L1, R1...
     * This drains the host-side samples but never changes the emulated sound chip's state.
     */
    drain_audio(): Float32Array;
    frame_count(): number;
    /**
     * RGBA8 pixels, copied across the wasm-to-JS boundary.
     */
    frame_rgba(): Uint8Array;
    height(): number;
    load_state(state: Uint8Array): void;
    /**
     * Input is the user's local Japanese Rev 1 or Klepto 1.02 ROM. ROM/IPS never leave the device.
     */
    constructor(rom: Uint8Array, ips: Uint8Array);
    save_state(): Uint8Array;
    /**
     * AudioContext.sampleRate controls the emulator's PCM output rate.
     */
    set_audio_rate(hz: number): void;
    /**
     * The bit layout is given by ssd_oracle::input::JOYPAD_BUTTONS.
     */
    tick(joypad_bits: number): void;
    width(): number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_webgame_free: (a: number, b: number) => void;
    readonly webgame_drain_audio: (a: number) => [number, number];
    readonly webgame_frame_count: (a: number) => number;
    readonly webgame_frame_rgba: (a: number) => [number, number];
    readonly webgame_height: (a: number) => number;
    readonly webgame_load_state: (a: number, b: number, c: number) => [number, number];
    readonly webgame_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly webgame_save_state: (a: number) => [number, number];
    readonly webgame_set_audio_rate: (a: number, b: number) => [number, number];
    readonly webgame_tick: (a: number, b: number) => [number, number];
    readonly webgame_width: (a: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
