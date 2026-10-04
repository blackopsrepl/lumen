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
    engine.load(QUrl::fromLocalFile(QString::fromLocal8Bit(argv[1])));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    return application.exec();
}
