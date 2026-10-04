// lumen: the viewer and settings application.
//
// It owns no sessions and no compositor. It connects to lumen-daemon over two
// local sockets — one for control, one for frames — and renders what the daemon
// sends. Closing this window ends nothing but the view.

#include "daemonclient.h"
#include "daemoncontrol.h"
#include "theme.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QStandardPaths>
#include <QTextStream>

namespace {

// QML load failures are not reliably visible on stderr in every launch context
// (a launcher or a service swallows them), so every engine diagnostic is also
// written to a file. Without this, failing to load the window is
// indistinguishable from any other instant exit.
QFile g_log;
QTextStream g_logStream;

void messageHandler(QtMsgType, const QMessageLogContext &, const QString &message) {
    QTextStream(stderr) << message << '\n';
    if (g_logStream.device()) {
        g_logStream << message << '\n';
        g_logStream.flush();
    }
}

} // namespace

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("lumen"));
    app.setOrganizationName(QStringLiteral("lumen"));
    app.setApplicationVersion(QStringLiteral("0.14.0"));

    const QString logPath =
        QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + "/viewer.log";
    QDir().mkpath(QFileInfo(logPath).absolutePath());
    g_log.setFileName(logPath);
    g_log.open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text);
    g_logStream.setDevice(&g_log);
    qInstallMessageHandler(messageHandler);

    // Both objects the QML chrome refers to are registered here. A context
    // property the QML names but the engine does not have is a load failure,
    // and the window then never appears at all.
    DaemonClient client;
    DaemonControl control;

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("daemon"), &client);
    engine.rootContext()->setContextProperty(QStringLiteral("daemonControl"), &control);
    // The palette is handed to QML as a context property rather than as a
    // module type. Whether a QML-side singleton resolves depends on module
    // metadata, and when it does not every colour is silently undefined and the
    // window renders in the default palette; a context property cannot fail
    // that way, and it stays `Theme.bg` to the QML either way.
    engine.rootContext()->setContextProperty(QStringLiteral("Theme"), Theme::instance());
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, []() {
        qCritical("lumen: QML object creation failed");
    });
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, &app,
                     [](const QList<QQmlError> &warnings) {
                         for (const QQmlError &warning : warnings) {
                             qCritical("lumen: %s", qPrintable(warning.toString()));
                         }
                     });
    engine.loadFromModule("Lumen", "Main");
    if (engine.rootObjects().isEmpty()) {
        qCritical("lumen: no root objects; the window could not be created");
        return 1;
    }
    qInfo("lumen: viewer started");
    return app.exec();
}
