// lumen-daemon: the resident Lumen service.
//
// Runs with no window, ever. It owns the compositor, every session, the
// accessibility walk, the feedback inbox and the agent socket, and it stays up
// when no viewer is attached — closing the viewer must not end a session an
// agent is driving.
//
// It is started and stopped as a systemd user service, not as a child of the
// GUI: a daemon that dies with the window that spawned it is not a daemon.

#include "accessibility.h"
#include "agentserver.h"
#include "compositor.h"
#include "config.h"
#include "feedbackstore.h"
#include "framestream.h"
#include "sessionmanager.h"

#include <QCommandLineParser>
#include <QDir>
#include <QFile>
#include <QGuiApplication>

int main(int argc, char** argv) {
    // The compositor needs the GUI stack (Wayland protocol objects, QImage) but
    // must never show a window. It is driven by an offscreen platform when the
    // host has no display of its own.
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("lumen-daemon"));
    app.setOrganizationName(QStringLiteral("lumen"));
    app.setApplicationVersion(QStringLiteral("0.14.0"));
    // Closing a viewer must not stop the daemon; it has no windows to close.
    app.setQuitOnLastWindowClosed(false);

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Lumen session daemon"));
    parser.addHelpOption();
    QCommandLineOption socketOption(QStringLiteral("socket"),
                                    QStringLiteral("Compositor socket name"),
                                    QStringLiteral("name"), QStringLiteral("lumen"));
    parser.addOption(socketOption);
    parser.process(app);

    Config* config = Config::load(&app);
    QDir().mkpath(config->profilesDir());
    QFile marker(config->profilesDir() + QStringLiteral("/.lumen-profile-root"));
    if (!marker.exists()) {
        marker.open(QIODevice::WriteOnly);
        marker.write("lumen ephemeral session profiles; safe to clear\n");
        marker.close();
    }

    auto* compositor = new LumenCompositor(&app);
    if (!compositor->start(parser.value(socketOption))) {
        qCritical("lumen-daemon: could not take the compositor socket");
        return 1;
    }

    auto* sessions = new SessionManager(config, compositor, &app);
    sessions->reconcileProfiles();

    auto* feedback = new FeedbackStore(config->feedbackDbPath(), &app);
    auto* agentServer = new AgentServer(sessions, feedback, compositor, &app);
    if (!agentServer->listen(config->agentSocketPath())) {
        qCritical("lumen-daemon: could not take the agent socket");
        return 1;
    }

    // Publish the socket paths for anything that runs inside a session. A
    // session's XDG_RUNTIME_DIR is its own private directory, so a nested
    // client — including another Lumen viewer — cannot derive the daemon's
    // paths from it and would look in the wrong place.
    qputenv("LUMEN_AGENT_SOCKET", config->agentSocketPath().toUtf8());
    qputenv("LUMEN_STREAM_SOCKET", config->streamSocketPath().toUtf8());

    // Viewers attach here. The daemon keeps running with none attached.
    auto* frames = new FrameStream(compositor, &app);
    if (!frames->listen(config->streamSocketPath())) {
        qCritical("lumen-daemon: could not take the stream socket");
        return 1;
    }

    // A session's title is reported by the compositor and recorded on the
    // session, so the viewer's list shows what the client calls itself.
    QObject::connect(compositor, &LumenCompositor::titleChanged, sessions,
                     [sessions](const QString& session, const QString& title) {
                         sessions->setTitle(session, title);
                     });

    qInfo("lumen-daemon: listening on %s", qPrintable(agentServer->path()));
    return app.exec();
}
