// End-to-end test of what an agent reads from a session — its frame and its
// item tree: a real session started by the session manager, the real agent
// socket, the real `lumen-cli` binary, and the real probe plugin in the client.
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
#include "itemtree.h"
#include "sessionmanager.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QImage>
#include <QJsonArray>
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
    void windows_are_placed_by_pid_and_decorated_size();

  private:
    /// Run `lumen-cli` against `socket` without blocking the event loop the
    /// in-process daemon answers on.
    static int runCli(const QString& socket, const QStringList& arguments, QByteArray* out,
                      QByteArray* err);
    static bool waitUntil(const std::function<bool()>& done, int ms);
    static QRect rectOf(const QJsonObject& node) {
        const QJsonArray rect = node[QStringLiteral("rect")].toArray();
        return QRect(rect.at(0).toInt(), rect.at(1).toInt(), rect.at(2).toInt(),
                     rect.at(3).toInt());
    }
    /// The first node of `node`'s subtree, depth first, that `match` accepts.
    static QJsonObject findItem(const QJsonObject& node,
                                const std::function<bool(const QJsonObject&)>& match) {
        if (match(node)) {
            return node;
        }
        for (const QJsonValue& child : node[QStringLiteral("children")].toArray()) {
            const QJsonObject found = findItem(child.toObject(), match);
            if (!found.isEmpty()) {
                return found;
            }
        }
        return QJsonObject();
    }

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

    // The item tree, from the probe inside the client, placed in the frame.
    QVERIFY2(runCli(socket, {QStringLiteral("tree"), QStringLiteral("framed")}, &out, &err) == 0,
             err.constData());
    const QJsonArray windows =
        QJsonDocument::fromJson(out).object()[QStringLiteral("windows")].toArray();
    QCOMPARE(windows.size(), 1);
    const QJsonObject window = windows.first().toObject();
    QVERIFY(window[QStringLiteral("mapped")].toBool());
    const QJsonObject root = window[QStringLiteral("root")].toObject();
    const QRect rootRect = rectOf(root);
    QVERIFY(!rootRect.isEmpty());
    // Placed right: the window's own fill is where the tree says the window is.
    const QImage settled = compositor.frame(QStringLiteral("framed"));
    QCOMPARE(settled.pixelColor(rootRect.x() + rootRect.width() / 4,
                                rootRect.y() + rootRect.height() / 2),
             QColor(QStringLiteral("#203040")));
    // Items the accessibility tree does not carry: an unnamed Text with its
    // properties, and a plain Item known by its QML id.
    const QJsonObject label = findItem(root, [](const QJsonObject& node) {
        return node[QStringLiteral("props")].toObject()[QStringLiteral("text")].toString() ==
               QLatin1String("SPIKE");
    });
    QCOMPARE(label[QStringLiteral("type")].toString(), QStringLiteral("Text"));
    QCOMPARE(label[QStringLiteral("props")]
                 .toObject()[QStringLiteral("font")]
                 .toObject()[QStringLiteral("pixelSize")]
                 .toInt(),
             48);
    QVERIFY(qAbs(rectOf(label).center().x() - rootRect.center().x()) <= 1);
    QVERIFY(!findItem(root, [](const QJsonObject& node) {
                 return node[QStringLiteral("id")].toString() == QLatin1String("keySink");
             }).isEmpty());
    // Its coordinates are the ones click takes.
    const QPoint centre = rectOf(label).center();
    QCOMPARE(runCli(socket,
                    {QStringLiteral("click"), QStringLiteral("framed"), QString::number(centre.x()),
                     QString::number(centre.y())},
                    &out, &err),
             0);
    QCOMPARE(runCli(socket, {QStringLiteral("tree"), QStringLiteral("absent")}, &out, &err), 1);

    sessions.stopAll();
}

// A panel is a layer surface away from the frame's origin, and a decorated
// window's buffer is larger than the window by its margins. The probe knows
// neither; placement must, or every coordinate an agent clicks is off.
void TestAgentFrame::windows_are_placed_by_pid_and_decorated_size() {
    const QJsonObject item{{QStringLiteral("type"), QStringLiteral("Text")},
                           {QStringLiteral("rect"), QJsonArray{10, 20, 30, 40}}};
    const auto window = [&](int width, int height, const QJsonArray& margins) {
        return QJsonObject{{QStringLiteral("size"), QJsonArray{width, height}},
                           {QStringLiteral("margins"), margins},
                           {QStringLiteral("root"),
                            QJsonObject{{QStringLiteral("rect"), QJsonArray{0, 0, width, height}},
                                        {QStringLiteral("children"), QJsonArray{item}}}}};
    };
    const QJsonObject process{
        {QStringLiteral("pid"), 42},
        {QStringLiteral("windows"), QJsonArray{window(1280, 1068, QJsonArray{0, 0, 0, 0}),
                                               window(400, 300, QJsonArray{5, 30, 5, 5}),
                                               window(200, 100, QJsonArray{0, 0, 0, 0})}}};
    const QList<LumenCompositor::Placement> placements{
        {7, QPoint(0, 0), QSize(1280, 1068)},     // another process, same size
        {42, QPoint(320, 52), QSize(1280, 1068)}, // the panel
        {42, QPoint(0, 0), QSize(410, 335)},      // the decorated window
    };
    const QJsonArray placed =
        ItemTree::place({process}, placements)[QStringLiteral("windows")].toArray();
    QCOMPARE(placed.size(), 3);

    const QJsonObject panel = placed.at(0).toObject();
    QVERIFY(panel[QStringLiteral("mapped")].toBool());
    QCOMPARE(panel[QStringLiteral("origin")].toArray(), (QJsonArray{320, 52}));
    QCOMPARE(rectOf(panel[QStringLiteral("root")]
                        .toObject()[QStringLiteral("children")]
                        .toArray()
                        .first()
                        .toObject()),
             QRect(330, 72, 30, 40));

    const QJsonObject decorated = placed.at(1).toObject();
    QCOMPARE(decorated[QStringLiteral("origin")].toArray(), (QJsonArray{5, 30}));

    // Not drawn: window coordinates, and said so.
    const QJsonObject unmapped = placed.at(2).toObject();
    QVERIFY(!unmapped[QStringLiteral("mapped")].toBool());
    QCOMPARE(rectOf(unmapped[QStringLiteral("root")]
                        .toObject()[QStringLiteral("children")]
                        .toArray()
                        .first()
                        .toObject()),
             QRect(10, 20, 30, 40));

    // The accessibility tree's window-relative bounds follow the same window.
    const auto bounds = [](int x, int y, int w, int h) {
        return QJsonObject{{QStringLiteral("x"), x},
                           {QStringLiteral("y"), y},
                           {QStringLiteral("width"), w},
                           {QStringLiteral("height"), h}};
    };
    QJsonObject tree{
        {QStringLiteral("role"), QStringLiteral("desktop frame")},
        {QStringLiteral("children"),
         QJsonArray{QJsonObject{
             {QStringLiteral("role"), QStringLiteral("application")},
             {QStringLiteral("children"),
              QJsonArray{QJsonObject{
                  {QStringLiteral("role"), QStringLiteral("frame")},
                  {QStringLiteral("bounds"), bounds(0, 0, 1280, 1068)},
                  {QStringLiteral("children"),
                   QJsonArray{QJsonObject{{QStringLiteral("role"), QStringLiteral("push button")},
                                          {QStringLiteral("bounds"), bounds(1138, 16, 65, 26)}},
                              QJsonObject{{QStringLiteral("role"), QStringLiteral("push button")},
                                          {QStringLiteral("bounds"), bounds(0, 0, 0, 0)}}}}}}}}}}};
    QCOMPARE(ItemTree::translateAccessibility(&tree, ItemTree::place({process}, placements)), 1);
    const QJsonArray buttons = tree[QStringLiteral("children")]
                                   .toArray()
                                   .first()
                                   .toObject()[QStringLiteral("children")]
                                   .toArray()
                                   .first()
                                   .toObject()[QStringLiteral("children")]
                                   .toArray();
    QCOMPARE(buttons.at(0).toObject()[QStringLiteral("bounds")].toObject(),
             bounds(1458, 68, 65, 26));
    // A control that is not on screen has nowhere to move to.
    QCOMPARE(buttons.at(1).toObject()[QStringLiteral("bounds")].toObject(), bounds(0, 0, 0, 0));
}

QTEST_MAIN(TestAgentFrame)
#include "test_agentframe.moc"
