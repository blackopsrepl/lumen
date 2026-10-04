#include "sessionmanager.h"

#include "compositor.h"
#include "config.h"
#include "session.h"

#include <QDir>
#include <QFileInfo>

SessionManager::SessionManager(Config* config, LumenCompositor* compositor, QObject* parent)
    : QObject(parent), m_config(config), m_compositor(compositor) {}

SessionManager::~SessionManager() {
    stopAll();
}

QVariantList SessionManager::sessions() const {
    QVariantList list;
    for (const QString& name : m_order) {
        Session* session = m_sessions.value(name);
        if (!session) {
            continue;
        }
        QVariantMap entry;
        entry[QStringLiteral("name")] = session->name();
        entry[QStringLiteral("command")] = session->command();
        entry[QStringLiteral("state")] = session->state();
        entry[QStringLiteral("title")] = session->title();
        entry[QStringLiteral("owner")] = session->owner();
        entry[QStringLiteral("agentOwned")] = session->agentOwned();
        entry[QStringLiteral("accessibilityReady")] = session->accessibilityReady();
        list.append(entry);
    }
    return list;
}

QString SessionManager::profileDir(const QString& name) const {
    return m_config->profilesDir() + QLatin1Char('/') + name;
}

QString SessionManager::create(const QString& name, const QString& command, bool agentOwned,
                               const QString& owner, QString* error) {
    const auto fail = [error](const QString& message) {
        if (error) {
            *error = message;
        }
        return QString();
    };
    if (!Config::isValidSessionName(name)) {
        return fail(QStringLiteral("invalid session name"));
    }
    if (!Config::isValidCommand(command)) {
        return fail(QStringLiteral("command must be an absolute path to an executable"));
    }
    if (m_sessions.contains(name)) {
        // Reusing a name whose session died is normal; a live one is a conflict.
        Session* existing = m_sessions.value(name);
        if (existing && existing->state() == QLatin1String("running")) {
            return fail(QStringLiteral("session '%1' is already running").arg(name));
        }
        stop(name);
    }
    if (m_sessions.size() >= m_config->maxSessions()) {
        return fail(QStringLiteral("session limit reached"));
    }

    auto* session = new Session(
        name, command, agentOwned ? Session::Origin::Agent : Session::Origin::Manual, owner, this);
    // The compositor identifies a client by its process id, so the session's
    // process must be registered *before* the client connects: the toplevel is
    // created during startup, and an unregistered pid means the surface is never
    // attributed to its session.
    if (!session->start(socketName(), profileDir(name), m_config->dbusBin(),
                        m_config->resolveRegistryd(), true)) {
        session->deleteLater();
        return fail(QStringLiteral("session '%1' failed to start").arg(name));
    }
    if (m_compositor) {
        m_compositor->expectProcess(name, session->processId());
    }
    m_sessions.insert(name, session);
    m_order.append(name);
    emit sessionsChanged();
    return name;
}

bool SessionManager::stop(const QString& name) {
    Session* session = m_sessions.take(name);
    if (!session) {
        return false;
    }
    session->stop();
    m_order.removeAll(name);
    session->deleteLater();
    // The profile exists only while its session does.
    QDir(profileDir(name)).removeRecursively();
    emit sessionsChanged();
    return true;
}

void SessionManager::stopAll() {
    const QList<QString> names = m_order;
    for (const QString& name : names) {
        stop(name);
    }
}

Session* SessionManager::session(const QString& name) const {
    return m_sessions.value(name);
}

void SessionManager::setTitle(const QString& name, const QString& title) {
    Session* session = m_sessions.value(name);
    if (!session) {
        return;
    }
    session->setTitle(title);
    emit sessionsChanged();
}

QString SessionManager::nameForPid(const QString& pid) const {
    return m_compositor ? m_compositor->sessionForPid(pid) : QString();
}

QString SessionManager::processIdFor(const QString& name) const {
    Session* session = m_sessions.value(name);
    return session ? QString::number(session->processId()) : QString();
}

void SessionManager::reconcileProfiles() {
    // The profile tree is service-owned and ephemeral: nothing in it may
    // outlive a session, so it is cleared at startup. A marker file gates the
    // recursive delete, so a misconfigured data_dir can never make this
    // destructive.
    const QString root = m_config->profilesDir();
    const QString marker = root + QStringLiteral("/.lumen-profile-root");
    if (!QFileInfo::exists(marker)) {
        return;
    }
    QDir dir(root);
    const QStringList entries = dir.entryList(QDir::Dirs | QDir::NoDotAndDotDot);
    for (const QString& entry : entries) {
        QDir(root + QLatin1Char('/') + entry).removeRecursively();
    }
}
