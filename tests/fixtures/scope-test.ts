// TypeScript file for testing tree-sitter scopes

// IMPORT statements
import { hello } from './module';
import defaultExport from 'some-module';
import * as fs from 'fs';

// TYPE definitions
interface User {
    name: string;
    age: number;
}

type HelloAlias = string;

// Function definitions
function helperFunction(x: number): number {
    return x * 2;
}

const arrowHelper = (x: number): number => x * 3;

async function asyncHelper(x: number): Promise<number> {
    return await Promise.resolve(x * 4);
}

// Class with methods
class UserClass implements User {
    name: string;
    age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }

    greet(): void {
        // Comment about hello
        console.log(`Hello, ${this.name}`);
    }

    helperMethod(): number {
        return this.age * 2;
    }
}

// Function calls
const result = helperFunction(42);
const user = new UserClass("Alice", 30);
user.greet();
console.log("hello from main");

// String literals
const greeting: string = "hello in string";
const template = `hello ${user.name}`;

// Tests (Jest style)
describe('UserClass', () => {
    it('should greet correctly', () => {
        const user = new UserClass('Test', 25);
        expect(user.name).toBe('Test');
    });

    test('hello helper method works', () => {
        const user = new UserClass('Test', 10);
        expect(user.helperMethod()).toBe(20);
    });
});
