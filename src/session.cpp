#include "session.h"

#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusReply>

#include <QDeadlineTimer>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QProcessEnvironment>
#include <QStandardPaths>
#include <QThread>
#include <QTimer>

#include <csignal>

namespace {

/// Wait until the private session bus has created its socket.
bool waitForFile(const QString& path, QProcess* child, int timeoutMs) {
    QDeadlineTimer deadline(timeoutMs);
    while (!deadline.hasExpired()) {
        if (QFileInfo::exists(path)) {
            return true;
        }
        if (child && child->state() == QProcess::NotRunning) {
            return false;
        }
        QThread::msleep(25);
    }
    return false;
}

} // namespace

Session::Session(const QString& name, const QString& command, Origin origin, const QString& owner,
                 QObject* parent)
    : QObject(parent), m_name(name), m_command(command), m_origin(origin), m_owner(owner) {}

Session::~Session() {
    stop();
}

qint64 Session::processId() const {
    return m_process ? m_process->processId() : 0;
}

void Session::setTitle(const QString& title) {
    if (m_title == title) {
        return;
    }
    m_title = title;
    emit titleChanged();
}

void Session::setState(const QString& state) {
    if (m_state == state) {
        return;
    }
    m_state = state;
    emit stateChanged();
}

void Session::setAccessibilityReady(bool ready) {
    if (m_accessibilityReady == ready) {
        return;
    }
    m_accessibilityReady = ready;
    emit accessibilityReadyChanged();
}

bool Session::start(const QString& socketName, const QString& profileDir, const QString& dbusBin,
                    const QString& registryd, bool accessibility) {
    m_profileDir = profileDir;
    QDir().mkpath(profileDir);
    // XDG_RUNTIME_DIR must be private to the session: 0700, or Qt refuses it and
    // every runtime path resolves somewhere shared instead.
    QFile::setPermissions(profileDir,
                          QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);

    // The compositor's socket is named by an absolute path, not a bare name: a
    // session runs with its own XDG_RUNTIME_DIR (its private bus lives there),
    // so a relative WAYLAND_DISPLAY would resolve against the wrong directory
    // and the client would fail to connect.
    const QString display =
        socketName.startsWith(QLatin1Char('/'))
            ? socketName
            : QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation) + QLatin1Char('/') +
                  socketName;

    // A private session bus and an AT-SPI registry, started eagerly. The
    // registry cannot be relied on to start lazily: when the host runs systemd
    // the bus launcher delegates activation to systemd, which can only reach
    // the real session bus, never a private one — and the tree comes back empty.
    if (accessibility && !dbusBin.isEmpty()) {
        const QString busSocket = profileDir + QStringLiteral("/bus");
        m_busAddress = QStringLiteral("unix:path=") + busSocket;
        m_dbus = new QProcess(this);
        QProcessEnvironment busEnv = QProcessEnvironment::systemEnvironment();
        busEnv.insert(QStringLiteral("XDG_RUNTIME_DIR"), profileDir);
        m_dbus->setProcessEnvironment(busEnv);
        m_dbus->start(dbusBin, {QStringLiteral("--session"), QStringLiteral("--nofork"),
                                QStringLiteral("--address=") + m_busAddress});
        if (!m_dbus->waitForStarted(5000) || !waitForFile(busSocket, m_dbus, 10000)) {
            setState(QStringLiteral("error"));
            m_busAddress.clear();
            return false;
        }
        if (!registryd.isEmpty()) {
            m_registryd = new QProcess(this);
            QProcessEnvironment regEnv = QProcessEnvironment::systemEnvironment();
            regEnv.insert(QStringLiteral("DBUS_SESSION_BUS_ADDRESS"), m_busAddress);
            regEnv.insert(QStringLiteral("XDG_RUNTIME_DIR"), profileDir);
            m_registryd->setProcessEnvironment(regEnv);
            m_registryd->start(registryd, {});
            m_registryd->waitForStarted(5000);
        }
    }

    m_process = new QProcess(this);
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(QStringLiteral("QT_QPA_PLATFORM"), QStringLiteral("wayland"));
    env.insert(QStringLiteral("WAYLAND_DISPLAY"), display);
    env.insert(QStringLiteral("XDG_RUNTIME_DIR"), profileDir);
    // The compositor reads a session's pixels out of the buffer the client
    // commits, and that only works for a shared-memory buffer. A client that
    // renders through GL commits a dmabuf, whose pixels the compositor cannot
    // map without a GL context — which a windowless daemon does not have — so
    // the frame comes back null and the session streams nothing.
    //
    // The rendering backend is therefore the session's, not the client's
    // choice: forcing the software rasterizer makes every session commit SHM
    // buffers, which is what makes the stream work for any Qt application
    // rather than only the ones that render in software already. It also keeps
    // sessions off the GPU, which is the right default for applications being
    // driven headlessly.
    env.insert(QStringLiteral("QT_QUICK_BACKEND"), QStringLiteral("software"));
    if (!m_busAddress.isEmpty()) {
        env.insert(QStringLiteral("DBUS_SESSION_BUS_ADDRESS"), m_busAddress);
        // Qt only keeps its AT-SPI bridge alive when told to; on a headless
        // surface it otherwise drops accessibility entirely.
        env.insert(QStringLiteral("QT_ACCESSIBILITY"), QStringLiteral("1"));
        env.insert(QStringLiteral("QT_LINUX_ACCESSIBILITY_ALWAYS_ON"), QStringLiteral("1"));
    }
    m_process->setProcessEnvironment(env);
    m_process->setWorkingDirectory(profileDir);
    m_process->setProcessChannelMode(QProcess::MergedChannels);

    // A session's own output is the only record of why it exited, so it is kept
    // beside the session rather than discarded into the daemon's stderr.
    m_outputFile = new QFile(profileDir + QStringLiteral("/session.log"), this);
    m_outputFile->open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text);

    connect(m_process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        if (m_outputFile) {
            // A process that exits immediately can leave its output in
            // the pipe buffer: drain what remains before recording.
            m_outputFile->write(m_process->readAllStandardOutput());
            m_outputFile->write(m_process->readAllStandardError());
            m_outputFile->write(QStringLiteral("exited with code %1, status %2\n")
                                    .arg(code)
                                    .arg(int(status))
                                    .toUtf8());
            m_outputFile->flush();
        }
        setState(code == 0 ? QStringLiteral("ended") : QStringLiteral("crashed"));
    });
    connect(m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (m_outputFile) {
            m_outputFile->write(QStringLiteral("process error %1: %2\n")
                                    .arg(int(error))
                                    .arg(m_process ? m_process->errorString() : QString())
                                    .toUtf8());
            m_outputFile->flush();
        }
    });
    connect(m_process, &QProcess::readyReadStandardOutput, this, [this]() {
        if (m_process && m_outputFile) {
            m_outputFile->write(m_process->readAllStandardOutput());
            m_outputFile->flush();
        }
    });

    const QStringList args = m_command.split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (args.isEmpty()) {
        setState(QStringLiteral("error"));
        return false;
    }
    m_process->start(args.first(), args.mid(1));
    if (!m_process->waitForStarted(8000)) {
        setState(QStringLiteral("error"));
        return false;
    }
    setState(QStringLiteral("running"));
    if (accessibility) {
        // The tree becomes readable once the application registers with the
        // registry; the accessibility walk reports emptiness until then.
        QTimer::singleShot(2000, this, [this]() { setAccessibilityReady(true); });
    }
    return true;
}

void Session::stop() {
    for (QProcess* child : {m_process, m_registryd, m_dbus}) {
        if (!child || child->state() == QProcess::NotRunning) {
            continue;
        }
        // Signals reach the child's whole process group, so a Qt application
        // that spawned helpers is torn down completely.
        const qint64 pid = child->processId();
        if (pid > 0) {
            ::kill(-static_cast<pid_t>(pid), SIGTERM);
        }
        child->terminate();
        if (!child->waitForFinished(3000)) {
            if (pid > 0) {
                ::kill(-static_cast<pid_t>(pid), SIGKILL);
            }
            child->kill();
            child->waitForFinished(3000);
        }
    }
    m_process = nullptr;
    m_registryd = nullptr;
    m_dbus = nullptr;
    setState(QStringLiteral("stopped"));
}
