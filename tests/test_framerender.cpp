// Test: does SessionView::frame() work headless?
//
// advance() moves to the client's latest buffer. currentBuffer().image()
// converts it to a QImage. Both may need a GL context, which may not exist
// in an offscreen environment. This test proves whether the daemon can
// render frames without a display.

#include <QtTest>
#include <QProcess>
#include <QProcessEnvironment>
#include <QEventLoop>
#include <QTimer>
#include <QDir>
#include <QStandardPaths>
#include <QImage>

#include "compositor.h"

class TestFrameRender : public QObject {
    Q_OBJECT

private slots:
    void frame_renders_headless();
};

void TestFrameRender::frame_renders_headless() {
    LumenCompositor compositor;
    QVERIFY(compositor.start("lumen-frame-test"));

    QString sessionName;
    bool surfaceReady = false;

    connect(&compositor, &LumenCompositor::surfaceReady, this, [&](const QString &session) {
        sessionName = session;
        surfaceReady = true;
    });

    QProcess fixture;
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    QString runtimeDir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtimeDir.isEmpty()) runtimeDir = QDir::tempPath();
    env.insert("WAYLAND_DISPLAY", runtimeDir + "/lumen-frame-test");
    env.insert("QT_QPA_PLATFORM", "wayland");
    env.insert("XDG_RUNTIME_DIR", runtimeDir);
    fixture.setProcessEnvironment(env);
    fixture.start(QCoreApplication::applicationDirPath() + "/qt_fixture",
                  QStringList() << QCoreApplication::applicationDirPath() + "/../src/spike/fixture.qml");
    compositor.expectProcess("fixture", fixture.processId());
    QVERIFY(fixture.waitForStarted(5000));

    // Wait for surface.
    {
        QEventLoop loop;
        QTimer timeout;
        timeout.setSingleShot(true);
        timeout.setInterval(5000);
        connect(&timeout, &QTimer::timeout, &loop, &QEventLoop::quit);
        connect(&compositor, &LumenCompositor::surfaceReady, &loop, &QEventLoop::quit);
        timeout.start();
        loop.exec();
    }
    QVERIFY(surfaceReady);

    // Wait for a real frame: the first commits carry no content, so poll
    // until the compositor holds pixels or the timeout expires.
    QImage frame;
    {
        QEventLoop loop;
        QTimer timeout;
        timeout.setSingleShot(true);
        timeout.setInterval(8000);
        connect(&timeout, &QTimer::timeout, &loop, &QEventLoop::quit);
        QTimer poll;
        poll.setInterval(100);
        connect(&poll, &QTimer::timeout, &loop, [&]() {
            frame = compositor.frame("fixture");
            if (!frame.isNull()) {
                loop.quit();
            }
        });
        timeout.start();
        poll.start();
        loop.exec();
    }
    qDebug() << "frame isNull:" << frame.isNull();
    qDebug() << "frame size:" << frame.size();
    qDebug() << "frame format:" << frame.format();

    // The frame should not be null — the fixture has drawn.
    QVERIFY(!frame.isNull());
    QVERIFY(frame.width() > 0);
    QVERIFY(frame.height() > 0);

    fixture.terminate();
    fixture.waitForFinished(5000);
}

QTEST_MAIN(TestFrameRender)
#include "test_framerender.moc"
