// Phase 0 spike for the native Qt Lumen.
//
// Proves the one architectural bet the rewrite rests on: Lumen can be a Qt
// Wayland *compositor*, host a session application as a nested Wayland client,
// render that client's surface into its own scene graph, and deliver pointer
// input to it — with no screencopy protocol, no virtual-input protocol, and no
// privilege, which is what makes this possible on Sway/Hyprland where a normal
// client can do none of those things.
//
// The harness drives the whole loop and reports a verdict:
//   1. load the compositor
//   2. launch the session client against the compositor socket
//   3. wait for its toplevel to map into a WaylandQuickItem
//   4. synthesize a click inside the hosted surface
//   5. look for the marker file the client's button writes
//
// Proof of input is a marker file, not client stdout: console.log on this Qt
// build goes to the journal, and the file is independent of Qt logging.
//
// Exit 0 only if the click marker appears.

#include <QDebug>
#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QProcess>
#include <QProcessEnvironment>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTest>
#include <QTimer>

#include <cstdio>

#ifndef SPIKE_QML_PATH
#define SPIKE_QML_PATH "Spike.qml"
#endif
#ifndef FIXTURE_BIN
#define FIXTURE_BIN "qt_fixture"
#endif
#ifndef FIXTURE_QML
#define FIXTURE_QML "fixture.qml"
#endif

namespace {

constexpr char kSocketName[] = "lumen-spike";
constexpr char kClickedTitle[] = "SPIKE-FIXTURE-CLICKED";

void say(const QString &line) {
    std::fputs(qPrintable(line + QLatin1Char('\n')), stdout);
    std::fflush(stdout);
}

bool markerPresent(const char *path) {
    return QFile::exists(QString::fromLatin1(path));
}

} // namespace

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    Q_UNUSED(argc);
    Q_UNUSED(argv);

    QQmlApplicationEngine engine;
    engine.load(QUrl::fromLocalFile(QStringLiteral(SPIKE_QML_PATH)));
    if (engine.rootObjects().isEmpty()) {
        say(QStringLiteral("SPIKE FAIL: compositor QML did not load"));
        return 2;
    }
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window) {
        say(QStringLiteral("SPIKE FAIL: root object is not a window"));
        return 2;
    }
    say(QStringLiteral("SPIKE compositor up, socket=%1").arg(QLatin1String(kSocketName)));

    auto *client = new QProcess(&app);
    client->setProcessChannelMode(QProcess::MergedChannels);
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(QStringLiteral("QT_QPA_PLATFORM"), QStringLiteral("wayland"));
    env.insert(QStringLiteral("WAYLAND_DISPLAY"), QLatin1String(kSocketName));
    env.insert(QStringLiteral("QT_QUICK_BACKEND"), QStringLiteral("software"));
    client->setProcessEnvironment(env);

    auto *view = window->findChild<QQuickItem *>(QStringLiteral("view"));

    int exitCode = -1;
    auto finish = [&](int code, const QString &verdict) {
        say(verdict);
        exitCode = code;
        client->terminate();
        if (!client->waitForFinished(3000)) {
            client->kill();
            client->waitForFinished(3000);
        }
        QTimer::singleShot(0, &app, [&app, code]() { app.exit(code); });
    };

    bool surfaceSeen = false;
    bool clicked = false;
    bool clickSent = false;

    auto *poll = new QTimer(&app);
    QObject::connect(poll, &QTimer::timeout, [&]() {
        if (!surfaceSeen && window->property("surfaceReady").toBool()
            && window->property("surfaceW").toInt() > 0) {
            surfaceSeen = true;
            const int sw = window->property("surfaceW").toInt();
            const int sh = window->property("surfaceH").toInt();
            say(QStringLiteral("SPIKE surface mapped: client surface %1x%2, item %3x%4, window %5x%6")
                    .arg(sw)
                    .arg(sh)
                    .arg(view ? qRound(view->width()) : -1)
                    .arg(view ? qRound(view->height()) : -1)
                    .arg(window->width())
                    .arg(window->height()));
            QTimer::singleShot(900, [&]() {
                const QPoint target(sw / 2, sh / 2);
                say(QStringLiteral("SPIKE clicking inside surface at %1,%2").arg(target.x()).arg(target.y()));
                QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, target);
                clickSent = true;
            });
        }
        if (clickSent && !clicked
            && window->property("toplevelTitle").toString() == QLatin1String(kClickedTitle)) {
            clicked = true;
            poll->stop();
            say(QStringLiteral("SPIKE input delivered: client title is now '%1'")
                    .arg(window->property("toplevelTitle").toString()));
            finish(0, QStringLiteral("SPIKE PASS"));
        }
    });
    poll->start(50);

    QTimer::singleShot(250, [&]() {
        client->start(QStringLiteral(FIXTURE_BIN), {QStringLiteral(FIXTURE_QML)});
        if (!client->waitForStarted(3000)) {
            say(QStringLiteral("SPIKE FAIL: session client did not start"));
            app.exit(3);
        }
    });

    // Hard deadline: a spike that hangs is a spike that failed.
    QTimer::singleShot(25000, [&]() {
        if (clicked) {
            return;
        }
        poll->stop();
        say(QStringLiteral("SPIKE diagnostics: title='%1' clicked=%2 clickSent=%3 surfaceReady=%4")
                .arg(window->property("toplevelTitle").toString())
                .arg(clicked)
                .arg(clickSent)
                .arg(window->property("surfaceReady").toBool()));
        say(QStringLiteral("--- client log ---"));
        const QString log = QString::fromUtf8(client->readAllStandardOutput());
        say(log.isEmpty() ? QStringLiteral("(empty)") : log);
        finish(4, QStringLiteral("SPIKE FAIL: no input marker within 25s"));
    });

    app.exec();
    return exitCode;
}
