// The session registry: owns every running session, its profile tree, and the
// link to the compositor that renders and drives each session's surface.

#pragma once

#include <QHash>
#include <QList>
#include <QObject>
#include <QString>
#include <QVariantList>

class Config;
class LumenCompositor;
class Session;

class SessionManager : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList sessions READ sessions NOTIFY sessionsChanged)
    Q_PROPERTY(int count READ count NOTIFY sessionsChanged)

public:
    SessionManager(Config *config, LumenCompositor *compositor, QObject *parent = nullptr);
    ~SessionManager() override;

    /// The compositor socket every session connects to.
    static QString socketName() { return QStringLiteral("lumen"); }

    QVariantList sessions() const;
    int count() const { return m_sessions.size(); }

    /// Create and start a session. Returns the name, or an empty string with
    /// `error` set when the request is invalid or the application fails.
    QString create(const QString &name, const QString &command, bool agentOwned,
                   const QString &owner, QString *error = nullptr);
    bool stop(const QString &name);
    void stopAll();
    Session *session(const QString &name) const;

    /// The session owning process `pid`, or an empty string.
    Q_INVOKABLE QString nameForPid(const QString &pid) const;
    /// The process id of a session's application, as a string.
    Q_INVOKABLE QString processIdFor(const QString &name) const;

    /// Record a session's window title, reported by the compositor.
    void setTitle(const QString &name, const QString &title);

    /// Remove every profile directory this service owns, at startup.
    void reconcileProfiles();

signals:
    void sessionsChanged();

private:
    QString profileDir(const QString &name) const;

    Config *m_config;
    LumenCompositor *m_compositor;
    QHash<QString, Session *> m_sessions;
    QList<QString> m_order;
};
