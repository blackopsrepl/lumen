// Controls the lumen-daemon systemd user unit.
//
// The daemon is a service, not a child of the GUI: it must survive the viewer
// closing, restart on failure, and start at login. systemd --user gives all
// three, and D-Bus is how a desktop app talks to it.

#pragma once

#include <QObject>
#include <QString>

class QDBusInterface;

class DaemonControl : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool active READ active NOTIFY activeChanged)
    Q_PROPERTY(bool enabled READ enabled NOTIFY enabledChanged)

  public:
    explicit DaemonControl(QObject* parent = nullptr);

    static QString unitName() { return QStringLiteral("lumen.service"); }

    bool active() const { return m_active; }
    bool enabled() const { return m_enabled; }

    /// Query the unit's state.
    Q_INVOKABLE void refresh();
    /// Start the daemon.
    Q_INVOKABLE void start();
    /// Stop the daemon.
    Q_INVOKABLE void stop();
    /// Whether the daemon starts at login.
    Q_INVOKABLE void setEnabled(bool enabled);

  signals:
    void activeChanged();
    void enabledChanged();

  private:
    bool m_active = false;
    bool m_enabled = false;
};
