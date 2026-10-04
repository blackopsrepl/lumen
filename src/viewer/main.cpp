// lumen: the viewer and settings application.
//
// It owns no sessions and no compositor. It connects to lumen-daemon over two
// local sockets — one for control, one for frames — and renders what the daemon
// sends. Closing this window ends nothing but the view.

#include "daemonclient.h"

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("lumen"));
    app.setOrganizationName(QStringLiteral("lumen"));
    app.setApplicationVersion(QStringLiteral("0.14.0"));

    DaemonClient client;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("daemon"), &client);
    engine.loadFromModule("Lumen", "Main");
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    return app.exec();
}
