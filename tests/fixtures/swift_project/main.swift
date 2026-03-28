import Foundation

/// A greeting function.
public func greet(name: String) -> String {
    return "Hello, \(name)!"
}

/// An animal class.
class Animal {
    var name: String

    init(name: String) {
        self.name = name
    }

    func speak() -> String {
        return name
    }
}

/// A point struct.
struct Point {
    var x: Double
    var y: Double
}

/// A drawable protocol.
protocol Drawable {
    func draw()
    func resize(width: Int, height: Int)
}

/// Direction enum.
enum Direction {
    case north
    case south
    case east
    case west
}

/// String extensions.
extension String {
    func shout() -> String {
        return self.uppercased()
    }
}

private func helper() -> Bool {
    return true
}

fileprivate func fileHelper() -> Int {
    return 42
}

// Entry point
let animal = Animal(name: "Rex")
let message = greet(name: animal.speak())
