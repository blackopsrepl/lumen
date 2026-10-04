pragma Singleton
import QtQuick

// Lumen's palette and type scale.
//
// These are the exact values the web viewer used, so the native application is
// the same product rather than a lookalike: a warm near-neutral dark, an agent
// accent that is a desaturated cool signal, and brass for the human.
QtObject {
    // --- surfaces ---
    readonly property color bg: "#121110"
    readonly property color panel: "#191817"
    readonly property color panel2: "#0e0d0c"
    readonly property color raised: "#211f1d"
    readonly property color line: "#2a2825"
    readonly property color lineSoft: "#22201e"

    // --- ink ---
    readonly property color text: "#e8e6e1"
    readonly property color muted: "#a09a90"
    readonly property color faint: "#6e6960"

    // --- signal ---
    readonly property color accent: "#a3c2c8"
    readonly property color accentInk: "#101d1f"
    readonly property color human: "#d8a04e"
    readonly property color humanInk: "#231804"
    readonly property color danger: "#d9776a"

    readonly property int radius: 4

    // The vendored IBM Plex family; falls back to the platform monospace if the
    // font is not installed, exactly as the stylesheet's stack did.
    readonly property string fontMono: "IBM Plex Mono"
    readonly property string fontSans: "IBM Plex Sans"

    readonly property int fontSize: 13
    readonly property int fontSizeSmall: 11
    readonly property int fontSizeTitle: 15
}
