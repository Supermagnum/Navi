package no.navi.app

import androidx.test.core.app.ActivityScenario
import org.junit.rules.TestRule
import org.junit.runner.Description
import org.junit.runners.model.Statement

/**
 * Launches MainActivity for Compose UI campaigns and leaves it running on teardown
 * (does not call scenario.close()). OD campaign helper; not production code.
 */
class LaunchMainActivityKeepAliveRule : TestRule {
    private var scenarioField: ActivityScenario<MainActivity>? = null
    val scenario: ActivityScenario<MainActivity>
        get() = scenarioField ?: error("MainActivity scenario not launched")

    fun requireActivity(): MainActivity {
        lateinit var act: MainActivity
        scenario.onActivity { act = it }
        return act
    }

    override fun apply(
        base: Statement,
        description: Description,
    ): Statement =
        object : Statement() {
            override fun evaluate() {
                scenarioField = ActivityScenario.launch(MainActivity::class.java)
                try {
                    base.evaluate()
                } finally {
                    // Leave MainActivity running — do not scenario.close().
                }
            }
        }
}
