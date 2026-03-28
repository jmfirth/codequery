/** An animal class. */
class Animal(val name: String) {
  def speak(): String = name

  def greet(): String = s"I am $name"
}

/** A drawable trait. */
trait Drawable {
  def draw(): Unit
  def resize(width: Int, height: Int): Unit
}

/** A singleton object. */
object Config {
  val maxRetries = 3
  val timeout = 30

  def reset(): Unit = {}
}

/** A case class for points. */
case class Point(x: Double, y: Double)

private class Secret {
  def hidden(): String = "secret"
}

protected trait Guarded {
  def check(): Boolean
}
