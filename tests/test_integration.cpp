// Integration test: prove that LumenCompositor::click() delivers input to a
// hosted Wayland client.
//
// This is the test that matters. The spike proves the architecture works (a
// compositor can host a client and QTest can click it). This test proves the
// daemon's input path works: LumenCompositor::click() uses the seat to deliver
// a mouse press to a client, and the client's response (a title change) is
// observed back through the compositor.
//
// The fixture is an ordinary Wayland client that changes its window title when
// clicked. The title is observed through the compositor's toplevel, so the test
// verifies the full round trip: click -> client -> title change -> compositor.

#include "compositor.h"

#include <QDir>
#include <QEventLoop>
#include <QProcess>
#include <QProcessEnvironment>
#include <QStandardPaths>
#include <QTimer>
#include <QtTest>

class TestIntegration : public QObject {
    Q_OBJECT

  private slots:
    void click_reaches_the_client();
};

void TestIntegration::click_reaches_the_client() {
    LumenCompositor compositor;
    QVERIFY(compositor.start("lumen-test"));

    QString sessionName;
    bool clicked = false;

    connect(&compositor, &LumenCompositor::surfaceReady, this,
            [&](const QString& session) { sessionName = session; });

    connect(&compositor, &LumenCompositor::titleChanged, this,
            [&](const QString&, const QString& title) {
                if (title == QLatin1String("SPIKE-FIXTURE-CLICKED")) {
                    clicked = true;
                }
            });

    // Start the fixture client. It connects to the compositor via WAYLAND_DISPLAY.
    QProcess fixture;
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    // Use an absolute path so the fixture finds the compositor socket regardless
    // of its own XDG_RUNTIME_DIR.
    QString runtimeDir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtimeDir.isEmpty())
        runtimeDir = QDir::tempPath();
    env.insert("WAYLAND_DISPLAY", runtimeDir + "/lumen-test");
    env.insert("QT_QPA_PLATFORM", "wayland");
    env.insert("XDG_RUNTIME_DIR", runtimeDir);
    fixture.setProcessEnvironment(env);
    fixture.start(QCoreApplication::applicationDirPath() + "/qt_fixture",
                  QStringList() << QCoreApplication::applicationDirPath() +
                                       "/../src/spike/fixture.qml");
    // Register the pid immediately after start, before the fixture connects.
    compositor.expectProcess("fixture", fixture.processId());
    QVERIFY(fixture.waitForStarted(5000));

    // Wait for the surface to appear.
    {
        QEventLoop loop;
        QTimer timeout;
        timeout.setSingleShot(true);
        timeout.setInterval(5000);
        connect(&timeout, &QTimer::timeout, &loop, &QEventLoop::quit);
        connect(&compositor, &LumenCompositor::surfaceReady, &loop, &QEventLoop::quit);
        timeout.start();
        loop.exec();
        qDebug() << "surfaceReady fired, session:" << sessionName;
        qDebug() << "fixture state:" << fixture.state();
        qDebug() << "fixture pid:" << fixture.processId();
    }

    // The surface should be registered under the name we registered.
    QVERIFY(compositor.viewFor("fixture") != nullptr);

    // Click in the center of the fixture (480x320).
    QVERIFY(compositor.click("fixture", QPointF(240, 160)));

    // Wait for the title to change.
    {
        QEventLoop loop;
        QTimer timeout;
        timeout.setSingleShot(true);
        timeout.setInterval(5000);
        connect(&timeout, &QTimer::timeout, &loop, &QEventLoop::quit);
        connect(&compositor, &LumenCompositor::titleChanged, &loop,
                [&](const QString&, const QString& title) {
                    if (title == QLatin1String("SPIKE-FIXTURE-CLICKED")) {
                        loop.quit();
                    }
                });
        timeout.start();
        loop.exec();
    }

    QVERIFY(clicked);

    fixture.terminate();
    fixture.waitForFinished(5000);
}

QTEST_MAIN(TestIntegration)
#include "test_integration.moc"
