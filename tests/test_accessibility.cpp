#include "accessibility.h"

#include <QJsonArray>
#include <QJsonObject>
#include <QTest>

namespace {

QJsonObject node(const QString& role, const QString& name, bool withBounds = true, int x = 0,
                 int y = 0, int w = 10, int h = 10) {
    QJsonObject object;
    object[QStringLiteral("role")] = role;
    object[QStringLiteral("name")] = name;
    if (withBounds) {
        QJsonObject bounds;
        bounds[QStringLiteral("x")] = x;
        bounds[QStringLiteral("y")] = y;
        bounds[QStringLiteral("width")] = w;
        bounds[QStringLiteral("height")] = h;
        object[QStringLiteral("bounds")] = bounds;
    }
    object[QStringLiteral("children")] = QJsonArray();
    return object;
}

void addChild(QJsonObject& parent, const QJsonObject& child) {
    QJsonArray children = parent[QStringLiteral("children")].toArray();
    children.append(child);
    parent[QStringLiteral("children")] = children;
}

} // namespace

// The counting rules, exercised against the exact tree shapes the live walk
// produces. These are the rules an agent relies on to know whether a tree is
// worth reading: a window-spanning filler is not an addressable control, and a
// window title is not a control's identity.
class TestAccessibility : public QObject {
    Q_OBJECT

  private slots:
    void counts_named_and_interior_controls_inside_a_window() {
        // desktop root > application > frame > filler > (label, text, button)
        QJsonObject button = node(QStringLiteral("push button"), QStringLiteral("Increment"), true,
                                  180, 180, 80, 24);
        QJsonObject text =
            node(QStringLiteral("text"), QStringLiteral("Name field"), true, 180, 144, 120, 24);
        QJsonObject label =
            node(QStringLiteral("label"), QStringLiteral("0"), true, 180, 116, 8, 16);
        QJsonObject filler = node(QStringLiteral("filler"), QString(), true, 0, 0, 480, 320);
        addChild(filler, label);
        addChild(filler, text);
        addChild(filler, button);

        QJsonObject frame =
            node(QStringLiteral("frame"), QStringLiteral("Lumen Qt Fixture"), true, 0, 0, 480, 320);
        addChild(frame, filler);

        QJsonObject app = node(QStringLiteral("application"), QStringLiteral("qt_a11y_fixture"),
                               true, 0, 0, 0, 0);
        addChild(app, frame);

        QJsonObject root = node(QStringLiteral("desktop frame"), QString());
        addChild(root, app);

        const QJsonObject stats = Accessibility::computeStats(root);
        QCOMPARE(stats[QStringLiteral("applications")].toInt(), 1);
        // application, frame, filler, label, text, button.
        QCOMPARE(stats[QStringLiteral("nodes")].toInt(), 6);
        // The window title does not count: only controls inside the window.
        QCOMPARE(stats[QStringLiteral("named")].toInt(), 3);
        // Every control has a rectangle of its own; the filler spans the window.
        QCOMPARE(stats[QStringLiteral("interior")].toInt(), 3);
    }

    void ignores_registry_process_names() {
        // The desktop root and the application entry always carry names; they
        // are registry chrome, not published content.
        QJsonObject app = node(QStringLiteral("application"), QStringLiteral("qt-loader"));
        addChild(app, node(QStringLiteral("frame"), QString()));
        QJsonObject root = node(QStringLiteral("desktop frame"), QStringLiteral("main"));
        addChild(root, app);

        const QJsonObject stats = Accessibility::computeStats(root);
        QCOMPARE(stats[QStringLiteral("named")].toInt(), 0);
        QCOMPARE(stats[QStringLiteral("interior")].toInt(), 0);
    }

    void reports_nothing_published_for_a_chrome_only_application() {
        // A bare rectangle publishes no accessible object at all; the filler
        // spans the window exactly, so nothing is addressable.
        QJsonObject frame = node(QStringLiteral("frame"), QString(), true, 0, 0, 480, 320);
        addChild(frame, node(QStringLiteral("filler"), QString(), true, 0, 0, 480, 320));
        QJsonObject app = node(QStringLiteral("application"), QStringLiteral("qt-loader"));
        addChild(app, frame);
        QJsonObject root = node(QStringLiteral("desktop frame"), QString());
        addChild(root, app);

        const QJsonObject stats = Accessibility::computeStats(root);
        QCOMPARE(stats[QStringLiteral("interior")].toInt(), 0);
        QCOMPARE(stats[QStringLiteral("named")].toInt(), 0);
    }

    void counts_unnamed_controls_as_addressable_interior() {
        // A titled window over an unnamed icon-only button: the title names the
        // window, not the button, so nothing is named — but the button is a
        // real object with its own rectangle, so the tree stays addressable.
        QJsonObject filler = node(QStringLiteral("filler"), QString(), true, 0, 0, 480, 320);
        addChild(filler, node(QStringLiteral("push button"), QString(), true, 226, 146, 28, 28));
        QJsonObject frame =
            node(QStringLiteral("frame"), QStringLiteral("Painted Canvas"), true, 0, 0, 480, 320);
        addChild(frame, filler);
        QJsonObject app = node(QStringLiteral("application"), QStringLiteral("qt-loader"));
        addChild(app, frame);
        QJsonObject root = node(QStringLiteral("desktop frame"), QString());
        addChild(root, app);

        const QJsonObject stats = Accessibility::computeStats(root);
        QCOMPARE(stats[QStringLiteral("named")].toInt(), 0);
        QCOMPARE(stats[QStringLiteral("interior")].toInt(), 1);
    }

    void reports_nothing_for_an_empty_registry() {
        // The registry keeps only its own root once the application departs.
        QJsonObject root = node(QStringLiteral("desktop frame"), QString());
        const QJsonObject stats = Accessibility::computeStats(root);
        QCOMPARE(stats[QStringLiteral("applications")].toInt(), 0);
        QCOMPARE(stats[QStringLiteral("interior")].toInt(), 0);
    }
};

QTEST_MAIN(TestAccessibility)
#include "test_accessibility.moc"
