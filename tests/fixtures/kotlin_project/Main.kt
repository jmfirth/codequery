import kotlin.math.PI
/** A greeting function. */
fun greet(name: String): String = "Hello, $name!"

/** An animal class. */
class Animal(val name: String) {
    fun speak(): String = name

    fun greet(): String = "I am $name"
}

/** A singleton object. */
object Config {
    val maxRetries = 3
    val timeout = 30

    fun reset() {}
}

/** A drawable interface. */
interface Drawable {
    fun draw()
    fun resize(width: Int, height: Int)
}

/** A data class for points. */
data class Point(val x: Double, val y: Double)

/** Direction enum. */
enum class Direction {
    NORTH,
    SOUTH,
    EAST,
    WEST
}

private fun helper(): Boolean = true

internal fun internalHelper(): Int = 42

fun main() {
    val animal = Animal("Rex")
    println(greet(animal.speak()))
}
