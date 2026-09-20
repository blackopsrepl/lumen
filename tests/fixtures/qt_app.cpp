// A minimal Qt Quick application for the accessibility E2E test.
//
// It publishes a few named controls and one that changes the tree when
// clicked, so the suite can prove that reading the tree and acting on it by
// reference both work against a real Qt process. The software renderer keeps
// it runnable on a headless host with no GPU.

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
