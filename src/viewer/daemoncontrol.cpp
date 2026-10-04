#include "daemoncontrol.h"

#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusReply>

namespace {

const char* kSystemdService = "org.freedesktop.systemd1";
const char* kSystemdPath = "/org/freedesktop/systemd1";
const char* kManagerInterface = "org.freedesktop.systemd1.Manager";
const char* kUnitInterface = "org.freedesktop.systemd1.Unit";

QDBusInterface* manager() {
    static QDBusInterface iface(kSystemdService, kSystemdPath, kManagerInterface,
                                QDBusConnection::sessionBus());
    return &iface;
}

QString unitPath() {
    QDBusReply<QDBusObjectPath> reply =
        manager()->call(QStringLiteral("GetUnit"), DaemonControl::unitName());
    return reply.isValid() ? reply.value().path() : QString();
}

} // namespace

DaemonControl::DaemonControl(QObject* parent) : QObject(parent) {
    // React to the unit changing state from anywhere, not just from this app.
    QDBusConnection::sessionBus().connect(kSystemdService, kSystemdPath, kManagerInterface,
                                          QStringLiteral("JobRemoved"), this, SLOT(refresh()));
}

void DaemonControl::refresh() {
    const QString path = unitPath();
    if (path.isEmpty()) {
        if (m_active) {
            m_active = false;
            emit activeChanged();
        }
        return;
    }
    QDBusInterface unit(kSystemdService, path, kUnitInterface, QDBusConnection::sessionBus());
    const bool active = unit.property("ActiveState").toString() == QLatin1String("active");
    const bool enabled = unit.property("UnitFileState").toString() == QLatin1String("enabled");
    if (active != m_active) {
        m_active = active;
        emit activeChanged();
    }
    if (enabled != m_enabled) {
        m_enabled = enabled;
        emit enabledChanged();
    }
}

void DaemonControl::start() {
    manager()->call(QStringLiteral("StartUnit"), unitName(), QStringLiteral("replace"));
}

void DaemonControl::stop() {
    manager()->call(QStringLiteral("StopUnit"), unitName(), QStringLiteral("replace"));
}

void DaemonControl::setEnabled(bool enabled) {
    manager()->call(enabled ? QStringLiteral("EnableUnitFiles")
                            : QStringLiteral("DisableUnitFiles"),
                    QVariantList{QVariantList{unitName(), false}}, false);
}
