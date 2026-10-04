// One agent session: a Qt application hosted by Lumen's compositor.
//
// Lumen owns the process and the profile directory it runs in. The application
// is an ordinary Wayland client pointed at Lumen's own socket; it knows nothing
// about Lumen. Because Lumen *is* the compositor, the application's surface is
// rendered straight into Lumen's scene graph and input is routed by Lumen's own
// seat — there is no capture protocol and no input injection anywhere.

#pragma once

#include <QObject>
#include <QFile>
#include <QPointer>
#include <QProcess>
#include <QString>

class Session : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString name READ name CONSTANT)
    Q_PROPERTY(QString command READ command CONSTANT)
    Q_PROPERTY(QString owner READ owner CONSTANT)
    Q_PROPERTY(bool agentOwned READ agentOwned CONSTANT)
    Q_PROPERTY(QString state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString title READ title NOTIFY titleChanged)
    Q_PROPERTY(QString busAddress READ busAddress CONSTANT)
    Q_PROPERTY(bool accessibilityReady READ accessibilityReady NOTIFY accessibilityReadyChanged)

public:
    enum class Origin { Agent, Manual };
    Q_ENUM(Origin)

    explicit Session(const QString &name, const QString &command, Origin origin,
                     const QString &owner, QObject *parent = nullptr);
    ~Session() override;

    /// Start the application under the compositor's socket, in its own profile.
    bool start(const QString &socketName, const QString &profileDir,
               const QString &dbusBin, const QString &registryd,
               bool accessibility);
    /// Ask the application to exit, then make sure it did.
    void stop();

    QString name() const { return m_name; }
    QString command() const { return m_command; }
    QString owner() const { return m_owner; }
    bool agentOwned() const { return m_origin == Origin::Agent; }
    QString state() const { return m_state; }
    QString title() const { return m_title; }
    QString busAddress() const { return m_busAddress; }
    bool accessibilityReady() const { return m_accessibilityReady; }
    qint64 processId() const;

    void setTitle(const QString &title);
    void setState(const QString &state);

signals:
    void stateChanged();
    void titleChanged();
    void accessibilityReadyChanged();

private:
    void setAccessibilityReady(bool ready);

    QString m_name;
    QString m_command;
    Origin m_origin;
    QString m_owner;
    QString m_state = QStringLiteral("starting");
    QString m_title;
    QString m_busAddress;
    QString m_profileDir;
    bool m_accessibilityReady = false;

    QProcess *m_process = nullptr;
    QProcess *m_dbus = nullptr;
    QProcess *m_registryd = nullptr;
    QFile *m_outputFile = nullptr;
};
