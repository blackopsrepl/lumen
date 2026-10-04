// Accessibility trees for Qt sessions.
//
// A Qt application publishes a structured tree of its controls on the
// accessibility bus: role, name, description and screen bounds. That is what
// lets an agent address a control by identity instead of estimating pixel
// coordinates from a screenshot. Each session has its own D-Bus connection, so
// a walk is scoped to that session's application.

#pragma once

#include <QJsonObject>
#include <QString>

class Accessibility {
public:
    /// Walk the accessibility tree of the application on `busAddress`.
    ///
    /// Returns the tree as JSON with a `stats` object, or an empty object with
    /// `error` set when nothing addressable was published.
    static QJsonObject tree(const QString &busAddress, QString *error);

    /// Wait until the session's accessibility registry answers.
    static bool awaitRegistry(const QString &busAddress, int timeoutMs);

    /// The raw walk, for diagnosing an empty tree.
    static QJsonObject debugWalk(const QString &busAddress);

    /// Measure what an application published, given the registry's root node.
    /// Exposed so the counting rules can be tested against real tree shapes.
    static QJsonObject computeStats(const QJsonObject &root);
};
