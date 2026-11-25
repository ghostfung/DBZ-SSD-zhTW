/* @ts-self-types="./ssd_web.d.ts" */
import * as import1 from "./snippets/ssd-web-78d3277d0c70cc4b/inline0.js"


export class WebGame {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebGameFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webgame_free(ptr, 0);
    }
    /**
     * Matches use the live PPU registers and VRAM content hash.
     * @returns {string}
     */
    background_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_background_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Detect stale Safari JS/WASM mixes; contains no user or ROM information.
     * @returns {string}
     */
    browser_build_tag() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_browser_build_tag(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    card_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_card_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    card_probe() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_card_probe(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Query the first-phase gameplay cheats. Never exposes ROM, RAM or save data.
     * @returns {string}
     */
    cheat_state() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_cheat_state(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {boolean}
     */
    classic_zh_mode() {
        const ret = wasm.webgame_classic_zh_mode(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Returns the original translator's keyed dialogue as JSON for testing.
     * This does not redraw the SNES picture: the companion browser UI displays
     * a non-destructive Chinese subtitle until the HD overlay is ported.
     * @returns {string}
     */
    current_dialogue() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_current_dialogue(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Interleaved stereo PCM, f32 in [-1, 1]: L0, R0, L1, R1...
     * This drains the host-side samples but never changes the emulated sound chip's state.
     * @returns {Float32Array}
     */
    drain_audio() {
        const ret = wasm.webgame_drain_audio(this.__wbg_ptr);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {string}
     */
    fighter_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_fighter_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Read-only counts explaining why a given stance has no local HD art.
     * @returns {string}
     */
    fighter_probe() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_fighter_probe(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    frame_count() {
        const ret = wasm.webgame_frame_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * RGBA8 pixels, copied across the wasm-to-JS boundary.
     * @returns {Uint8Array}
     */
    frame_rgba() {
        const ret = wasm.webgame_frame_rgba(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {boolean}
     */
    hd_active() {
        const ret = wasm.webgame_hd_active(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    hd_art_pixels() {
        const ret = wasm.webgame_hd_art_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    hd_background_pending() {
        const ret = wasm.webgame_hd_background_pending(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    hd_card_pixels() {
        const ret = wasm.webgame_hd_card_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * SNES pixels with confirmed visible fighter OAM pixels replaced.
     * @returns {number}
     */
    hd_fighter_pixels() {
        const ret = wasm.webgame_hd_fighter_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    hd_map_pixels() {
        const ret = wasm.webgame_hd_map_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    hd_mode7_pixels() {
        const ret = wasm.webgame_hd_mode7_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    hd_mode() {
        const ret = wasm.webgame_hd_mode(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    hd_pending_mask() {
        const ret = wasm.webgame_hd_pending_mask(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    hd_portrait_pixels() {
        const ret = wasm.webgame_hd_portrait_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Bitmask of visible HD art still awaiting local ZIP image decoding.
     * This prevents six expensive JS/WASM JSON roundtrips on *every* frame
     * while preserving same-frame requests when a new match appears.
     * Average and maximum sampled phase times since the last read. A
     * read resets counters, so each iPhone screenshot reflects the current
     * scene rather than an old battle stall.
     * @returns {string}
     */
    hd_profile_report() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_hd_profile_report(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    hd_special_pixels() {
        const ret = wasm.webgame_hd_special_pixels(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {string}
     */
    hd_warning() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_hd_warning(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    height() {
        const ret = wasm.webgame_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {Uint8Array} state
     */
    load_state(state) {
        const ptr0 = passArray8ToWasm0(state, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_load_state(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Load the original RC1 zh-TW translation TSV in memory without altering the ROM.
     * @param {string} tsv
     * @returns {number}
     */
    load_translation(tsv) {
        const ptr0 = passStringToWasm0(tsv, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_load_translation(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @returns {string}
     */
    map_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_map_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    map_probe() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_map_probe(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * The original project's menu and name translations, indexed by the English words on screen.
     * @returns {string}
     */
    menu_translations() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_menu_translations(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    mode7_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_mode7_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    mode7_probe() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_mode7_probe(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * True only when the original Rust translator painted the *current*
     * Enhanced picture; the browser must not add heuristic text overlays.
     * @returns {boolean}
     */
    native_translation_active() {
        const ret = wasm.webgame_native_translation_active(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Input is the user's local Japanese Rev 1 or Klepto 1.02 ROM. ROM/IPS never leave the device.
     * @param {Uint8Array} rom
     * @param {Uint8Array} ips
     */
    constructor(rom, ips) {
        const ptr0 = passArray8ToWasm0(rom, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(ips, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_new(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WebGameFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {string}
     */
    portrait_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_portrait_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    portrait_probe() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_portrait_probe(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    save_state() {
        const ret = wasm.webgame_save_state(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * AudioContext.sampleRate controls the emulator's PCM output rate.
     * @param {number} hz
     */
    set_audio_rate(hz) {
        const ret = wasm.webgame_set_audio_rate(this.__wbg_ptr, hz);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Accept a single locally decoded PNG/WebP of the matched battle background.
     * @param {number} index
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_background_art(index, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_background_art(this.__wbg_ptr, index, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Receive only the verified player's on-device background index (no media upload).
     * @param {string} json
     * @returns {number}
     */
    set_background_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_background_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} index
     * @param {number} layer
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_card_art(index, layer, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_card_art(this.__wbg_ptr, index, layer, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {boolean} enabled
     */
    set_card_hd_enabled(enabled) {
        wasm.webgame_set_card_hd_enabled(this.__wbg_ptr, enabled);
    }
    /**
     * Source-keyed official card and UI tile art, layers blended through
     * the ROM's PPU palettes (base and roles).
     * @param {string} json
     * @returns {number}
     */
    set_card_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_card_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Apply only the upstream's whitelisted gameplay settings. Changes are
     * limited to the running session and are not embedded in exported states.
     * @param {string} key
     * @param {string} value
     * @returns {string}
     */
    set_cheat_option(key, value) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            const ret = wasm.webgame_set_cheat_option(this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * Render real PPU pixels at 1x, with Traditional Chinese drawn by the
     * existing glyph-matched translator. No HD artwork is required.
     * @param {boolean} enabled
     */
    set_classic_zh_mode(enabled) {
        wasm.webgame_set_classic_zh_mode(this.__wbg_ptr, enabled);
    }
    /**
     * @param {number} index
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_fighter_art(index, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_fighter_art(this.__wbg_ptr, index, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * A/B compare exact same battle frame with background HD left on.
     * @param {boolean} enabled
     */
    set_fighter_hd_enabled(enabled) {
        wasm.webgame_set_fighter_hd_enabled(this.__wbg_ptr, enabled);
    }
    /**
     * Only the player's verified local class2 index; never HD bytes or ROM
     * data sent over a network. Exact fighter palette and hash are required.
     * @param {string} json
     * @returns {number}
     */
    set_fighter_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_fighter_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Safely decoded, official enlarged class2 pose; never a guessed overlay.
     * @param {number} index
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_fighter_variant(index, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_fighter_variant(this.__wbg_ptr, index, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Use upstream Enhanced 2x renderer; backgrounds are supplied by the player.
     * Unsupported frames fall back to Classic without affecting emulator state.
     * @param {boolean} enabled
     */
    set_hd_mode(enabled) {
        wasm.webgame_set_hd_mode(this.__wbg_ptr, enabled);
    }
    /**
     * @param {boolean} enabled
     */
    set_map_hd_enabled(enabled) {
        wasm.webgame_set_map_hd_enabled(this.__wbg_ptr, enabled);
    }
    /**
     * @param {string} json
     * @returns {number}
     */
    set_map_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_map_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} index
     * @param {number} x
     * @param {number} y
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_map_tile(index, x, y, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_map_tile(this.__wbg_ptr, index, x, y, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {boolean} on
     */
    set_mode7_hd_enabled(on) {
        wasm.webgame_set_mode7_hd_enabled(this.__wbg_ptr, on);
    }
    /**
     * @param {string} json
     * @returns {number}
     */
    set_mode7_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_mode7_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} index
     * @param {number} x
     * @param {number} y
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_mode7_tile(index, x, y, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_mode7_tile(this.__wbg_ptr, index, x, y, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {number} index
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_portrait_art(index, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_portrait_art(this.__wbg_ptr, index, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {boolean} enabled
     */
    set_portrait_hd_enabled(enabled) {
        wasm.webgame_set_portrait_hd_enabled(this.__wbg_ptr, enabled);
    }
    /**
     * Load validated class1 metadata; the palette-index source pixels are
     * reconstructed from the user's own ROM, never from remote services.
     * @param {string} json
     * @returns {number}
     */
    set_portrait_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_portrait_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {number} index
     * @param {number} width
     * @param {number} height
     * @param {Uint8Array} rgba
     */
    set_special_art(index, width, height, rgba) {
        const ptr0 = passArray8ToWasm0(rgba, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_special_art(this.__wbg_ptr, index, width, height, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {boolean} enabled
     */
    set_special_hd_enabled(enabled) {
        wasm.webgame_set_special_hd_enabled(this.__wbg_ptr, enabled);
    }
    /**
     * Only verified class3 sprites and class2 effects, never a named
     * character or a guessed screen rectangle.
     * @param {string} json
     * @returns {number}
     */
    set_special_index(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webgame_set_special_index(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {boolean} enabled
     */
    set_widescreen(enabled) {
        wasm.webgame_set_widescreen(this.__wbg_ptr, enabled);
    }
    /**
     * @returns {string}
     */
    special_matches() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_special_matches(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    special_probe() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_special_probe(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Whether a story dialogue frame contains untranslated PPU letters
     * that should be deferred briefly before Safari presents it.
     * @returns {boolean}
     */
    story_translation_pending() {
        const ret = wasm.webgame_story_translation_pending(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * The bit layout is given by ssd_oracle::input::JOYPAD_BUTTONS.
     * @param {number} joypad_bits
     */
    tick(joypad_bits) {
        const ret = wasm.webgame_tick(this.__wbg_ptr, joypad_bits);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * A compact, local-only diagnostic for rare missed English pages.
     * Contains no ROM/IPS/HD bytes and never sends data to a server.
     * @returns {string}
     */
    translation_diagnostics() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_translation_diagnostics(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * A direct lookup for an already-translated, token-free alert.
     * @param {string} key
     * @returns {string}
     */
    translation_for_key(key) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.webgame_translation_for_key(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * The glyph positions from the actual PPU frame (without any ROM modification).
     * @returns {string}
     */
    visible_text_cells() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webgame_visible_text_cells(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    width() {
        const ret = wasm.webgame_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) WebGame.prototype[Symbol.dispose] = WebGame.prototype.free;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_41e9ee4f547fc59a: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_getRandomValues_436a51d0629d84e1: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_now_aa4ccb83129e9e55: function() {
            const ret = Date.now();
            return ret;
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./ssd_web_bg.js": import0,
        "./snippets/ssd-web-78d3277d0c70cc4b/inline0.js": import1,
    };
}

const WebGameFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webgame_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedFloat32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('ssd_web_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
