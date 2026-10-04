// Lumen's process-wide configuration.
//
// A native application has no container to be configured by, so this is a
// plain ini file the user can edit, with sensible defaults. There is no
// environment-variable layering: a desktop app should not change its behavior
// based on what happens to be exported in the shell that launched it.

#pragma once

#include <QObject>
#include <QString>
#include <QStringList>

class Config : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString dataDir READ dataDir CONSTANT)
    Q_PROPERTY(QString dbusBin READ dbusBin CONSTANT)
    Q_PROPERTY(QString atSpiRegistryd READ atSpiRegistryd CONSTANT)
    Q_PROPERTY(QString swayBin READ swayBin CONSTANT)
    Q_PROPERTY(int defaultWidth READ defaultWidth CONSTANT)
    Q_PROPERTY(int defaultHeight READ defaultHeight CONSTANT)
    Q_PROPERTY(int maxSessions READ maxSessions CONSTANT)

  public:
    explicit Config(QObject* parent = nullptr);

    /// Load from the user's config file, falling back to defaults.
    static Config* load(QObject* parent = nullptr);

    QString dataDir() const { return m_dataDir; }
    QString dbusBin() const { return m_dbusBin; }
    QString atSpiRegistryd() const { return m_atSpiRegistryd; }
    QString swayBin() const { return m_swayBin; }
    int defaultWidth() const { return m_defaultWidth; }
    int defaultHeight() const { return m_defaultHeight; }
    int maxSessions() const { return m_maxSessions; }

    QString configPath() const;
    QString profilesDir() const;
    QString feedbackDbPath() const;
    QString agentSocketPath() const;
    /// Where viewers attach for the frame stream.
    QString streamSocketPath() const;

    /// Validate a session name: no path components, bounded length.
    static bool isValidSessionName(const QString& name);
    /// Validate a session command: absolute path to an executable file.
    static bool isValidCommand(const QString& command);

    /// Where the AT-SPI registry daemon lives, discovering it if unset.
    QString resolveRegistryd() const;

  private:
    QString m_dataDir;
    QString m_dbusBin;
    QString m_atSpiRegistryd;
    QString m_swayBin;
    int m_defaultWidth = 1440;
    int m_defaultHeight = 900;
    int m_maxSessions = 8;
};
