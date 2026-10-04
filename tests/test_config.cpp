#include "config.h"

#include <QTest>

class TestConfig : public QObject {
    Q_OBJECT

  private slots:
    void rejects_path_components_as_names() {
        QVERIFY(!Config::isValidSessionName(QStringLiteral(".")));
        QVERIFY(!Config::isValidSessionName(QStringLiteral("..")));
        QVERIFY(!Config::isValidSessionName(QStringLiteral("../escape")));
        QVERIFY(!Config::isValidSessionName(QStringLiteral("a/b")));
        QVERIFY(!Config::isValidSessionName(QString()));
    }

    void rejects_overlong_names() {
        QVERIFY(!Config::isValidSessionName(QString(33, QLatin1Char('a'))));
        QVERIFY(Config::isValidSessionName(QString(32, QLatin1Char('a'))));
    }

    void accepts_ordinary_names() {
        QVERIFY(Config::isValidSessionName(QStringLiteral("alice")));
        QVERIFY(Config::isValidSessionName(QStringLiteral("agent-1")));
        QVERIFY(Config::isValidSessionName(QStringLiteral("a_b.c")));
    }

    void rejects_relative_or_missing_commands() {
        QVERIFY(!Config::isValidCommand(QStringLiteral("ls")));
        QVERIFY(!Config::isValidCommand(QStringLiteral("relative/path")));
        QVERIFY(!Config::isValidCommand(QStringLiteral("/no/such/binary")));
        // A file that exists but is not executable is not a session command.
        QVERIFY(!Config::isValidCommand(QStringLiteral("/etc/hostname")));
    }

    void accepts_an_executable_absolute_path() {
        QVERIFY(Config::isValidCommand(QStringLiteral("/bin/sh")));
    }
};

QTEST_MAIN(TestConfig)
#include "test_config.moc"
