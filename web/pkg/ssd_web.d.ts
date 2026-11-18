/* tslint:disable */
/* eslint-disable */

export class WebGame {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Matches use the live PPU registers and VRAM content hash.
     */
    background_matches(): string;
    /**
     * Returns the original translator's keyed dialogue as JSON for testing.
     * This does not redraw the SNES picture: the companion browser UI displays
     * a non-destructive Chinese subtitle until the HD overlay is ported.
     */
    current_dialogue(): string;
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
    hd_active(): boolean;
    hd_art_pixels(): number;
    hd_mode(): boolean;
    hd_warning(): string;
    height(): number;
    load_state(state: Uint8Array): void;
    /**
     * Load the original RC1 zh-TW translation TSV in memory without altering the ROM.
     */
    load_translation(tsv: string): number;
    /**
     * The original project's menu and name translations, indexed by the English words on screen.
     */
    menu_translations(): string;
    /**
     * True only when the original Rust translator painted the *current*
     * Enhanced picture; the browser must not add heuristic text overlays.
     */
    native_translation_active(): boolean;
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
     * Accept a single locally decoded PNG/WebP of the matched battle background.
     */
    set_background_art(index: number, width: number, height: number, rgba: Uint8Array): void;
    /**
     * Receive only the verified player's on-device background index (no media upload).
     */
    set_background_index(json: string): number;
    /**
     * Use upstream Enhanced 2x renderer; backgrounds are supplied by the player.
     * Unsupported frames fall back to Classic without affecting emulator state.
     */
    set_hd_mode(enabled: boolean): void;
    /**
     * Whether a story dialogue frame contains untranslated PPU letters
     * that should be deferred briefly before Safari presents it.
     */
    story_translation_pending(): boolean;
    /**
     * The bit layout is given by ssd_oracle::input::JOYPAD_BUTTONS.
     */
    tick(joypad_bits: number): void;
    /**
     * A compact, local-only diagnostic for rare missed English pages.
     * Contains no ROM/IPS/HD bytes and never sends data to a server.
     */
    translation_diagnostics(): string;
    /**
     * A direct lookup for an already-translated, token-free alert.
     */
    translation_for_key(key: string): string;
    /**
     * The glyph positions from the actual PPU frame (without any ROM modification).
     */
    visible_text_cells(): string;
    width(): number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_webgame_free: (a: number, b: number) => void;
    readonly webgame_background_matches: (a: number) => [number, number];
    readonly webgame_current_dialogue: (a: number) => [number, number];
    readonly webgame_drain_audio: (a: number) => [number, number];
    readonly webgame_frame_count: (a: number) => number;
    readonly webgame_frame_rgba: (a: number) => [number, number];
    readonly webgame_hd_active: (a: number) => number;
    readonly webgame_hd_art_pixels: (a: number) => number;
    readonly webgame_hd_mode: (a: number) => number;
    readonly webgame_hd_warning: (a: number) => [number, number];
    readonly webgame_height: (a: number) => number;
    readonly webgame_load_state: (a: number, b: number, c: number) => [number, number];
    readonly webgame_load_translation: (a: number, b: number, c: number) => [number, number, number];
    readonly webgame_menu_translations: (a: number) => [number, number];
    readonly webgame_native_translation_active: (a: number) => number;
    readonly webgame_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly webgame_save_state: (a: number) => [number, number];
    readonly webgame_set_audio_rate: (a: number, b: number) => [number, number];
    readonly webgame_set_background_art: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly webgame_set_background_index: (a: number, b: number, c: number) => [number, number, number];
    readonly webgame_set_hd_mode: (a: number, b: number) => void;
    readonly webgame_story_translation_pending: (a: number) => number;
    readonly webgame_tick: (a: number, b: number) => [number, number];
    readonly webgame_translation_diagnostics: (a: number) => [number, number];
    readonly webgame_translation_for_key: (a: number, b: number, c: number) => [number, number];
    readonly webgame_visible_text_cells: (a: number) => [number, number];
    readonly webgame_width: (a: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
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
