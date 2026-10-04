// A minimal Qt Quick application used as the session client for the spike.
// It is an ordinary Wayland client: it knows nothing about Lumen.
#include <QByteArray>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QString>
#include <QUrl>

int main(int argc, char **argv) {
    qputenv("QT_QUICK_BACKEND", QByteArrayLiteral("software"));
    QGuiApplication application(argc, argv);
    QQmlApplicationEngine engine;
    // Report why a load failed rather than exiting silently: a bare "exit 1"
    // from a session is otherwise impossible to diagnose.
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &application,
                     []() { qCritical("fixture: QML object creation failed"); });
    if (argc < 2) {
        qCritical("fixture: no QML path given");
        return 1;
    }
    engine.load(QUrl::fromLocalFile(QString::fromLocal8Bit(argv[1])));
    if (engine.rootObjects().isEmpty()) {
        qCritical("fixture: loaded no root objects from %s", argv[1]);
        return 1;
    }
    return application.exec();
}
