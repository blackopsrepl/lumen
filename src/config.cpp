#include "config.h"

#include <QDir>
#include <QFileInfo>
#include <QSettings>
#include <QStandardPaths>

namespace {

constexpr int kMaxNameLength = 32;

} // namespace

Config::Config(QObject* parent) : QObject(parent) {
    const QString base = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    m_dataDir = base.isEmpty() ? QStringLiteral("/tmp/lumen") : base;
    m_dbusBin = QStringLiteral("dbus-daemon");
    m_swayBin = QStringLiteral("sway");
}

Config* Config::load(QObject* parent) {
    auto* config = new Config(parent);
    const QString path = config->configPath();
    if (!QFileInfo::exists(path)) {
        return config;
    }
    QSettings settings(path, QSettings::IniFormat);
    config->m_dataDir = settings.value(QStringLiteral("data_dir"), config->m_dataDir).toString();
    config->m_dbusBin = settings.value(QStringLiteral("dbus_bin"), config->m_dbusBin).toString();
    config->m_atSpiRegistryd =
        settings.value(QStringLiteral("at_spi_registryd"), config->m_atSpiRegistryd).toString();
    config->m_swayBin = settings.value(QStringLiteral("sway_bin"), config->m_swayBin).toString();
    config->m_defaultWidth =
        settings.value(QStringLiteral("default_width"), config->m_defaultWidth).toInt();
    config->m_defaultHeight =
        settings.value(QStringLiteral("default_height"), config->m_defaultHeight).toInt();
    config->m_maxSessions =
        settings.value(QStringLiteral("max_sessions"), config->m_maxSessions).toInt();
    return config;
}

QString Config::configPath() const {
    return QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation) +
           QStringLiteral("/lumen.ini");
}

QString Config::profilesDir() const {
    return m_dataDir + QStringLiteral("/run");
}

QString Config::feedbackDbPath() const {
    return m_dataDir + QStringLiteral("/feedback.db");
}

QString Config::agentSocketPath() const {
    // Unix socket paths have a hard length limit; keep it under the runtime
    // directory rather than deep inside the data tree.
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty()) {
        runtime = m_dataDir;
    }
    return runtime + QStringLiteral("/lumen-agent.sock");
}

QString Config::streamSocketPath() const {
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty()) {
        runtime = m_dataDir;
    }
    return runtime + QStringLiteral("/lumen-stream.sock");
}

bool Config::isValidSessionName(const QString& name) {
    if (name.isEmpty() || name.length() > kMaxNameLength) {
        return false;
    }
    if (name == QLatin1String(".") || name == QLatin1String("..")) {
        return false;
    }
    for (const QChar& ch : name) {
        if (!(ch.isLetterOrNumber() || ch == QLatin1Char('-') || ch == QLatin1Char('_') ||
              ch == QLatin1Char('.'))) {
            return false;
        }
    }
    return !name.startsWith(QLatin1Char('.'));
}

bool Config::isValidCommand(const QString& command) {
    // A session command is an absolute program plus arguments; the program is
    // what has to exist and be executable.
    const QStringList parts = command.split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (parts.isEmpty()) {
        return false;
    }
    const QString program = parts.first();
    if (!program.startsWith(QLatin1Char('/'))) {
        return false;
    }
    const QFileInfo info(program);
    return info.exists() && info.isFile() && info.isExecutable();
}

QString Config::resolveRegistryd() const {
    if (!m_atSpiRegistryd.isEmpty()) {
        return m_atSpiRegistryd;
    }
    // Where distributions install the registry daemon, in the order Debian,
    // openSUSE and Fedora use.
    const QStringList candidates = {
        QStringLiteral("at-spi2-registryd"),
        QStringLiteral("/usr/libexec/at-spi2-registryd"),
        QStringLiteral("/usr/libexec/at-spi2/at-spi2-registryd"),
        QStringLiteral("/usr/lib/at-spi2-core/at-spi2-registryd"),
    };
    for (const QString& candidate : candidates) {
        if (candidate.startsWith(QLatin1Char('/'))) {
            if (QFileInfo(candidate).isExecutable()) {
                return candidate;
            }
        } else {
            const QString found = QStandardPaths::findExecutable(candidate);
            if (!found.isEmpty()) {
                return found;
            }
        }
    }
    return QString();
}
