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
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QProcess>
#include <QProcessEnvironment>
#include <QStandardPaths>
#include <QTimer>
#include <QtTest>

#include <functional>

class TestIntegration : public QObject {
    Q_OBJECT

  private slots:
    // The compositor derives its Wayland socket path from XDG_RUNTIME_DIR, and
    // Qt refuses a runtime dir that is not mode 0700. On a desktop session the
    // variable is already set; inside a CI container it is not, and the
    // compositor then cannot open its socket. Give the test its own private
    // runtime dir so it does not depend on the ambient environment at all.
    void initTestCase() {
        if (qEnvironmentVariableIsEmpty("XDG_RUNTIME_DIR")) {
            QString dir = QDir::tempPath() + "/lumen-test-runtime";
            QDir().mkpath(dir);
            QFile::setPermissions(dir, QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                           QFileDevice::ExeOwner);
            qputenv("XDG_RUNTIME_DIR", dir.toUtf8());
        }
    }

    void click_reaches_the_client();
    void surface_is_attributed_through_a_launcher();

  private:
    /// Start the fixture on `qml` with the environment a session gives it.
    void startFixture(QProcess& process, const QString& socket, const QString& session,
                      const QString& program, const QStringList& arguments);
    /// Run the event loop until `done` holds or `ms` pass.
    static bool waitUntil(const std::function<bool()>& done, int ms);
};

void TestIntegration::startFixture(QProcess& process, const QString& socket, const QString& session,
                                   const QString& program, const QStringList& arguments) {
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    QString runtimeDir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtimeDir.isEmpty())
        runtimeDir = QDir::tempPath();
    env.insert("WAYLAND_DISPLAY", runtimeDir + "/" + socket);
    env.insert("QT_QPA_PLATFORM", "wayland");
    env.insert("XDG_RUNTIME_DIR", runtimeDir);
    env.insert(LumenCompositor::kSessionEnvVar, session);
    process.setProcessEnvironment(env);
    process.start(program, arguments);
}

bool TestIntegration::waitUntil(const std::function<bool()>& done, int ms) {
    QElapsedTimer timer;
    timer.start();
    while (!done() && timer.elapsed() < ms) {
        QTest::qWait(50);
    }
    return done();
}

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
                                       "/../tests/fixtures/fixture.qml");
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

// A session's command is often not the process that draws: a launcher script
// runs the application as a child, and some applications re-execute or
// daemonize. The connecting pid is then one nobody registered, and the window
// used to be filed under no session at all — the session reported "running",
// clicks returned ok:false, and the viewer showed nothing.
void TestIntegration::surface_is_attributed_through_a_launcher() {
    LumenCompositor compositor;
    QVERIFY(compositor.start("lumen-test-launcher"));

    // `; true` stops the shell from exec'ing the fixture in its own place, so
    // the fixture is a child with a pid of its own, as under a real launcher.
    const QString fixture = QCoreApplication::applicationDirPath() + "/qt_fixture";
    const QString qml = QCoreApplication::applicationDirPath() + "/../tests/fixtures/fixture.qml";
    QProcess launcher;
    startFixture(launcher, "lumen-test-launcher", "launched", "/bin/sh",
                 {"-c", QStringLiteral("'%1' '%2'; true").arg(fixture, qml)});
    QVERIFY(launcher.waitForStarted(5000));
    compositor.expectProcess("launched", launcher.processId());

    QVERIFY(waitUntil([&]() { return !compositor.frame("launched").isNull(); }, 8000));
    QVERIFY(compositor.click("launched", QPointF(240, 160)));
    QVERIFY(compositor.frame(QString()).isNull());

    // The fixture is the shell's child, so it is ended by parent pid.
    QProcess::execute("pkill", {"-P", QString::number(launcher.processId())});
    launcher.terminate();
    launcher.waitForFinished(5000);
}

QTEST_MAIN(TestIntegration)
#include "test_integration.moc"
