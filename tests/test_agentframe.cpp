// End-to-end test of the agent's frame read: a real session started by the
// session manager, the real agent socket, and the real `lumen-cli` binary.
//
// Without a way to read pixels from the session the human watches, an agent
// verifying what it drew stood up a private compositor to screenshot instead,
// and the viewer showed none of that work. This proves `lumen-cli frame` hands
// back exactly the session's composited frame, and that a large reply survives
// the many socket reads it arrives in.

#include "agentserver.h"
#include "compositor.h"
#include "config.h"
#include "feedbackstore.h"
#include "sessionmanager.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QImage>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QProcessEnvironment>
#include <QTemporaryDir>
#include <QtTest>

#include <functional>

class TestAgentFrame : public QObject {
    Q_OBJECT

  private slots:
    // Always a private runtime and data directory, even on a desktop: the
    // session manager's compositor socket is named `lumen`, which the running
    // daemon already owns in the real runtime directory.
    void initTestCase() {
        QVERIFY(m_root.isValid());
        const QString runtime = m_root.filePath(QStringLiteral("run"));
        QVERIFY(QDir().mkpath(runtime));
        QFile::setPermissions(runtime, QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                           QFileDevice::ExeOwner);
        qputenv("XDG_RUNTIME_DIR", runtime.toUtf8());
        qputenv("XDG_DATA_HOME", m_root.filePath(QStringLiteral("data")).toUtf8());
    }

    void frame_writes_the_sessions_composited_frame();

  private:
    /// Run `lumen-cli` against `socket` without blocking the event loop the
    /// in-process daemon answers on.
    static int runCli(const QString& socket, const QStringList& arguments, QByteArray* out,
                      QByteArray* err);
    static bool waitUntil(const std::function<bool()>& done, int ms);

    QTemporaryDir m_root;
};

bool TestAgentFrame::waitUntil(const std::function<bool()>& done, int ms) {
    QElapsedTimer timer;
    timer.start();
    while (!done() && timer.elapsed() < ms) {
        QTest::qWait(50);
    }
    return done();
}

int TestAgentFrame::runCli(const QString& socket, const QStringList& arguments, QByteArray* out,
                           QByteArray* err) {
    QProcess cli;
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(QStringLiteral("LUMEN_AGENT_SOCKET"), socket);
    cli.setProcessEnvironment(env);
    cli.start(QCoreApplication::applicationDirPath() + QStringLiteral("/lumen-cli"), arguments);
    if (!cli.waitForStarted(5000) ||
        !waitUntil([&]() { return cli.state() == QProcess::NotRunning; }, 15000)) {
        cli.kill();
        return -1;
    }
    *out = cli.readAllStandardOutput();
    *err = cli.readAllStandardError();
    return cli.exitStatus() == QProcess::NormalExit ? cli.exitCode() : -1;
}

void TestAgentFrame::frame_writes_the_sessions_composited_frame() {
    LumenCompositor compositor;
    QVERIFY(compositor.start(SessionManager::socketName()));
    Config config;
    SessionManager sessions(&config, &compositor);
    FeedbackStore feedback(m_root.filePath(QStringLiteral("feedback.db")));
    AgentServer server(&sessions, &feedback, &compositor);
    const QString socket = m_root.filePath(QStringLiteral("agent.sock"));
    QVERIFY(server.listen(socket));

    const QString app = QCoreApplication::applicationDirPath();
    QString error;
    const QString created =
        sessions.create(QStringLiteral("framed"),
                        QStringLiteral("%1/qt_fixture %1/../tests/fixtures/fixture.qml").arg(app),
                        true, QStringLiteral("test"), &error);
    QVERIFY2(!created.isEmpty(), qPrintable(error));
    // Wait for the settled frame: the client's first buffer predates its own
    // paint at the configured size. The probe point is the window's own fill,
    // left of the label at its centre.
    QVERIFY(waitUntil(
        [&]() {
            const QImage frame = compositor.frame(QStringLiteral("framed"));
            return !frame.isNull() && frame.pixelColor(frame.width() / 4, frame.height() / 2) ==
                                          QColor(QStringLiteral("#203040"));
        },
        10000));

    QByteArray out;
    QByteArray err;
    const QString png = m_root.filePath(QStringLiteral("frame.png"));
    QCOMPARE(runCli(socket, {QStringLiteral("frame"), QStringLiteral("framed"), png}, &out, &err),
             0);
    // The compositor configures a toplevel to the full output, so the frame
    // is output-sized: large enough that the reply spans many socket reads.
    const QSize expected = compositor.frame(QStringLiteral("framed")).size();
    const QJsonObject printed = QJsonDocument::fromJson(out).object();
    QCOMPARE(printed[QStringLiteral("path")].toString(), png);
    QCOMPARE(printed[QStringLiteral("width")].toInt(), expected.width());
    QCOMPARE(printed[QStringLiteral("height")].toInt(), expected.height());

    // The file is the frame the human sees, not a blank or a truncated image.
    const QImage image(png);
    QVERIFY(!image.isNull());
    QCOMPARE(image.size(), expected);
    QCOMPARE(image.convertToFormat(QImage::Format_RGB32),
             compositor.frame(QStringLiteral("framed")).convertToFormat(QImage::Format_RGB32));
    QCOMPARE(image.pixelColor(expected.width() / 4, expected.height() / 2),
             QColor(QStringLiteral("#203040")));

    // A frame that cannot be read is a failure with a reason, never exit 0.
    QCOMPARE(runCli(socket, {QStringLiteral("frame"), QStringLiteral("absent"), png}, &out, &err),
             1);
    QVERIFY2(err.contains("no such session"), err.constData());
    QCOMPARE(runCli(socket, {QStringLiteral("frame"), QStringLiteral("framed")}, &out, &err), 2);

    sessions.stopAll();
}

QTEST_MAIN(TestAgentFrame)
#include "test_agentframe.moc"
