// Lumen's palette and type scale.
//
// This is a C++ singleton rather than a QML one. A QML `pragma Singleton` is
// resolved through the module's generated metadata, and when that resolution
// fails the properties are simply undefined — the window renders in the default
// palette with no error of its own, which is a silent and very confusing
// failure. A registered instance either exists or the module fails to load, and
// the values are then available to QML exactly as `Theme.bg`.
//
// The values are the ones the web viewer's stylesheet used, so the native
// application is the same product rather than a lookalike: a warm near-neutral
// dark, a desaturated cool accent for the agent, and brass for the human.

#pragma once

#include <QColor>
#include <QObject>

class Theme : public QObject {
    Q_OBJECT

    Q_PROPERTY(QColor bg READ bg CONSTANT)
    Q_PROPERTY(QColor panel READ panel CONSTANT)
    Q_PROPERTY(QColor panel2 READ panel2 CONSTANT)
    Q_PROPERTY(QColor raised READ raised CONSTANT)
    Q_PROPERTY(QColor line READ line CONSTANT)
    Q_PROPERTY(QColor lineSoft READ lineSoft CONSTANT)

    Q_PROPERTY(QColor text READ text CONSTANT)
    Q_PROPERTY(QColor muted READ muted CONSTANT)
    Q_PROPERTY(QColor faint READ faint CONSTANT)

    Q_PROPERTY(QColor accent READ accent CONSTANT)
    Q_PROPERTY(QColor accentInk READ accentInk CONSTANT)
    Q_PROPERTY(QColor human READ human CONSTANT)
    Q_PROPERTY(QColor humanInk READ humanInk CONSTANT)
    Q_PROPERTY(QColor danger READ danger CONSTANT)

    // Interaction states. A control needs a resting, a hovered and a pressed
    // surface, plus a focus ring, or it reads as an unstyled default widget
    // sitting in the middle of a designed window.
    Q_PROPERTY(QColor hover READ hover CONSTANT)
    Q_PROPERTY(QColor press READ press CONSTANT)
    Q_PROPERTY(QColor lineStrong READ lineStrong CONSTANT)
    Q_PROPERTY(QColor focusRing READ focusRing CONSTANT)
    Q_PROPERTY(QColor scrim READ scrim CONSTANT)

    // A restrained chrome ramp. The reference is a soft machined surface, not a
    // heavy gradient: the stops differ by a few percent, enough to give a button
    // a lit top and a shaded bottom without the surface becoming the loudest
    // thing on screen. The tones sit on the palette's own neutrals.
    Q_PROPERTY(QColor metalTop READ metalTop CONSTANT)
    Q_PROPERTY(QColor metalMid READ metalMid CONSTANT)
    Q_PROPERTY(QColor metalBottom READ metalBottom CONSTANT)
    Q_PROPERTY(QColor metalEdge READ metalEdge CONSTANT)
    Q_PROPERTY(QColor metalSeam READ metalSeam CONSTANT)
    Q_PROPERTY(QColor metalTopActive READ metalTopActive CONSTANT)
    Q_PROPERTY(QColor metalBottomActive READ metalBottomActive CONSTANT)
    Q_PROPERTY(QColor wellTop READ wellTop CONSTANT)
    Q_PROPERTY(QColor wellBottom READ wellBottom CONSTANT)

    Q_PROPERTY(int radius READ radius CONSTANT)
    Q_PROPERTY(int radiusLarge READ radiusLarge CONSTANT)
    Q_PROPERTY(int controlHeight READ controlHeight CONSTANT)

    Q_PROPERTY(QString fontMono READ fontMono CONSTANT)
    Q_PROPERTY(QString fontSans READ fontSans CONSTANT)

    Q_PROPERTY(int fontSize READ fontSize CONSTANT)
    Q_PROPERTY(int fontSizeSmall READ fontSizeSmall CONSTANT)
    Q_PROPERTY(int fontSizeTitle READ fontSizeTitle CONSTANT)

  public:
    using QObject::QObject;

    static Theme* instance() {
        static Theme theme;
        return &theme;
    }

    QColor bg() const { return QColor(QStringLiteral("#121110")); }
    QColor panel() const { return QColor(QStringLiteral("#191817")); }
    QColor panel2() const { return QColor(QStringLiteral("#0e0d0c")); }
    QColor raised() const { return QColor(QStringLiteral("#211f1d")); }
    QColor line() const { return QColor(QStringLiteral("#2a2825")); }
    QColor lineSoft() const { return QColor(QStringLiteral("#22201e")); }

    QColor text() const { return QColor(QStringLiteral("#e8e6e1")); }
    QColor muted() const { return QColor(QStringLiteral("#a09a90")); }
    QColor faint() const { return QColor(QStringLiteral("#6e6960")); }

    QColor accent() const { return QColor(QStringLiteral("#a3c2c8")); }
    QColor accentInk() const { return QColor(QStringLiteral("#101d1f")); }
    QColor human() const { return QColor(QStringLiteral("#d8a04e")); }
    QColor humanInk() const { return QColor(QStringLiteral("#231804")); }
    QColor danger() const { return QColor(QStringLiteral("#d9776a")); }

    QColor hover() const { return QColor(QStringLiteral("#2c2a26")); }
    QColor press() const { return QColor(QStringLiteral("#171614")); }
    QColor lineStrong() const { return QColor(QStringLiteral("#3b3833")); }
    QColor focusRing() const { return QColor(163, 194, 200, 110); }
    QColor scrim() const { return QColor(8, 8, 7, 150); }

    // Soft machined chrome: the stops sit within a few percent of each other.
    QColor metalTop() const { return QColor(QStringLiteral("#26241f")); }
    QColor metalMid() const { return QColor(QStringLiteral("#201e1b")); }
    QColor metalBottom() const { return QColor(QStringLiteral("#1a1917")); }
    QColor metalEdge() const { return QColor(QStringLiteral("#3a3731")); }
    QColor metalSeam() const { return QColor(QStringLiteral("#141311")); }
    QColor metalTopActive() const { return QColor(QStringLiteral("#171614")); }
    QColor metalBottomActive() const { return QColor(QStringLiteral("#131211")); }
    // A well is recessed: darker than the shell, and inverted top-to-bottom.
    QColor wellTop() const { return QColor(QStringLiteral("#0d0c0b")); }
    QColor wellBottom() const { return QColor(QStringLiteral("#121110")); }

    int radius() const { return 6; }
    int radiusLarge() const { return 10; }
    int controlHeight() const { return 30; }

    QString fontMono() const { return QStringLiteral("IBM Plex Mono"); }
    QString fontSans() const { return QStringLiteral("IBM Plex Sans"); }

    int fontSize() const { return 13; }
    int fontSizeSmall() const { return 11; }
    int fontSizeTitle() const { return 15; }
};
