// Checks the package links and answers through SwiftPM (the way the Mac app depends on it). The thorough
// checks are in native/swift-test/main.swift, which build-mac.sh runs on both architectures.

import Foundation
import XCTest
@testable import QCalcCore

final class QCalcCoreTests: XCTestCase {
    let engine = Engine(config: EngineConfig())

    func request(_ text: String) -> LiveRequest {
        LiveRequest(text: text, options: defaultEvalOptions(), context: EvalContext(), nowMs: 0)
    }

    func testLiveAnswers() {
        let live = engine.live(request: request("1234567*2"))
        XCTAssertEqual(live.display, "2469134")
        XCTAssertEqual(live.pretty, "2,469,134")
        XCTAssertEqual(live.kind, .expression)
    }

    func testAsyncAndCancel() async throws {
        let lines = try await engine.evaluateAsync(lines: ["x = 3", "x^2"], options: defaultEvalOptions())
        XCTAssertEqual(lines.last?.display, "9")
        let token = CancelToken()
        token.cancel()
        do {
            _ = try await engine.evaluateAsync(lines: ["1+1"], options: defaultEvalOptions(), cancel: token)
            XCTFail("a cancelled token still answered")
        } catch CoreError.Cancelled {}
    }

    func testDeepInputOnASmallStack() {
        let deep = String(repeating: "(", count: 250) + "1" + String(repeating: ")", count: 250)
        let done = expectation(description: "answered")
        var shown = ""
        let thread = Thread { [engine] in
            shown = engine.live(request: LiveRequest(text: deep, options: defaultEvalOptions(),
                                                     context: EvalContext(), nowMs: 0)).display
            done.fulfill()
        }
        thread.stackSize = 512 * 1024
        thread.start()
        wait(for: [done], timeout: 10)
        XCTAssertEqual(shown, "1")
    }
}
