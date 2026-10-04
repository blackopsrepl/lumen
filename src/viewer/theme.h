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

    Q_PROPERTY(int radius READ radius CONSTANT)

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

    int radius() const { return 4; }

    QString fontMono() const { return QStringLiteral("IBM Plex Mono"); }
    QString fontSans() const { return QStringLiteral("IBM Plex Sans"); }

    int fontSize() const { return 13; }
    int fontSizeSmall() const { return 11; }
    int fontSizeTitle() const { return 15; }
};
