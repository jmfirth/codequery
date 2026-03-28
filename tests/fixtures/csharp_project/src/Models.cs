using System;

namespace MyApp.Models
{
    public class User
    {
        public string Name { get; set; }
        private int _age;

        public User(string name, int age)
        {
            Name = name;
            _age = age;
        }

        public string Greet()
        {
            ValidateAge();
            return $"Hello, {Name}";
        }

        private void InternalCheck() {}
        protected void ValidateAge() {}
    }

    public interface IGreeter
    {
        string Greet();
    }

    public struct Point
    {
        public int X;
        public int Y;
    }

    public enum Color
    {
        Red,
        Green,
        Blue
    }

    internal class InternalHelper {}
}
