import XCTest
@testable import SmoothMX

final class ConfigurationTests: XCTestCase {
    func testMouseActionAcceptsEveryPublicActionKind() throws {
        try MouseAction().validate()
        try MouseAction(kind: 1, code: 127, modifiers: 15).validate()
        try MouseAction(kind: 2, code: 4).validate()
        try MouseAction(kind: 4, code: 18).validate()
        try MouseAction(kind: 5).validate()
    }

    func testMouseActionRejectsReservedKindsAndUnusedFields() {
        let invalidActions = [
            MouseAction(kind: 0, code: 1),
            MouseAction(kind: 1, code: 128),
            MouseAction(kind: 1, modifiers: 16),
            MouseAction(kind: 2, code: 5),
            MouseAction(kind: 2, modifiers: 1),
            MouseAction(kind: 3),
            MouseAction(kind: 4, code: 2),
            MouseAction(kind: 4, code: 16, modifiers: 1),
            MouseAction(kind: 5, code: 1),
            MouseAction(kind: 6)
        ]

        for action in invalidActions {
            XCTAssertThrowsError(try action.validate(), "Unexpectedly accepted \(action)")
        }
    }

    func testSettingsValidateNestedActionsEvenWhenTheirControlIsDisabled() {
        var invalidHorizontal = MouseSettings()
        invalidHorizontal.horizontalActions[0] = MouseAction(kind: 3)
        XCTAssertThrowsError(try invalidHorizontal.validate())

        var invalidButton = MouseSettings()
        invalidButton.buttons[2].actions[4] = MouseAction(kind: 5, code: 1)
        XCTAssertThrowsError(try invalidButton.validate())

        var valid = MouseSettings()
        valid.horizontalActions = [MouseAction(kind: 5), MouseAction(kind: 1, code: 8, modifiers: 1)]
        valid.buttons[0].enabled = true
        valid.buttons[0].gesture = true
        valid.buttons[0].actions = [
            MouseAction(kind: 1, code: 6, modifiers: 1),
            MouseAction(kind: 2, code: 2),
            MouseAction(kind: 4, code: 16),
            MouseAction(kind: 5),
            MouseAction()
        ]
        XCTAssertNoThrow(try valid.validate())
    }

    func testSettingsRejectMalformedShapeAndNumericBounds() {
        var malformedButtons = MouseSettings()
        malformedButtons.buttons.removeLast()
        XCTAssertThrowsError(try malformedButtons.validate())

        var malformedActions = MouseSettings()
        malformedActions.buttons[0].actions.removeLast()
        XCTAssertThrowsError(try malformedActions.validate())

        var malformedHorizontal = MouseSettings()
        malformedHorizontal.horizontalActions.removeLast()
        XCTAssertThrowsError(try malformedHorizontal.validate())

        var invalidNumber = MouseSettings()
        invalidNumber.verticalSpeed = .nan
        XCTAssertThrowsError(try invalidNumber.validate())

        var invalidRange = MouseSettings()
        invalidRange.gestureThreshold = 2001
        XCTAssertThrowsError(try invalidRange.validate())
    }

    func testAccessibilityRequirementIsMinimalForInternalActions() {
        XCTAssertFalse(MouseAction(kind: 5).needsAccessibility)
        XCTAssertTrue(MouseAction(kind: 1, code: 8, modifiers: 1).needsAccessibility)
        XCTAssertTrue(MouseAction(kind: 2, code: 2).needsAccessibility)
        XCTAssertTrue(MouseAction(kind: 4, code: 16).needsAccessibility)

        var internalButton = MouseSettings()
        internalButton.buttons[0].enabled = true
        internalButton.buttons[0].actions[0] = MouseAction(kind: 5)
        XCTAssertFalse(internalButton.needsAccessibility)

        var verticalScroll = MouseSettings()
        verticalScroll.vertical = true
        XCTAssertTrue(verticalScroll.needsAccessibility)

        var passthroughHorizontalScroll = MouseSettings()
        passthroughHorizontalScroll.horizontal = true
        XCTAssertTrue(passthroughHorizontalScroll.needsAccessibility)

        var internalHorizontalActions = MouseSettings()
        internalHorizontalActions.horizontal = true
        internalHorizontalActions.horizontalActions = [MouseAction(kind: 5), MouseAction(kind: 5)]
        XCTAssertFalse(internalHorizontalActions.needsAccessibility)

        var shortcutButton = MouseSettings()
        shortcutButton.buttons[0].enabled = true
        shortcutButton.buttons[0].actions[0] = MouseAction(kind: 1, code: 8, modifiers: 1)
        XCTAssertTrue(shortcutButton.needsAccessibility)

        shortcutButton.buttons[0].enabled = false
        XCTAssertFalse(shortcutButton.needsAccessibility)
    }

    func testWordsHaveStableLengthAndFieldLayout() {
        var settings = MouseSettings()
        settings.dpi = 1600
        settings.smartShift = false
        settings.wheelMode = 1
        settings.threshold = 42
        settings.vertical = true
        settings.verticalSpeed = 125
        settings.horizontalActions[1] = MouseAction(kind: 1, code: 9, modifiers: 3)
        settings.buttons[4].enabled = true
        settings.buttons[4].gesture = true
        settings.buttons[4].actions[4] = MouseAction(kind: 4, code: 18)

        let words = settings.words
        XCTAssertEqual(words.count, 105)
        XCTAssertEqual(Array(words[0..<12]), [1, 1600, 1, 255, 1, 125, 0, 1, 0, 100, 0, 80])
        XCTAssertEqual(Array(words[15..<18]), [1, 9, 3])

        let lastButtonOffset = 20 + 4 * 17
        XCTAssertEqual(Array(words[lastButtonOffset..<(lastButtonOffset + 2)]), [1, 1])
        let lastActionOffset = lastButtonOffset + 2 + 4 * 3
        XCTAssertEqual(Array(words[lastActionOffset..<(lastActionOffset + 3)]), [4, 18, 0])
    }

    func testWordsEncodeSmartShiftThreshold() {
        var settings = MouseSettings()
        settings.smartShift = true
        settings.wheelMode = 1
        settings.threshold = 42

        XCTAssertEqual(settings.words[2], 2)
        XCTAssertEqual(settings.words[3], 42)
    }

    func testConfigurationRequiresExactlyOneGlobalAndUniqueProfileIDs() throws {
        let app = ApplicationProfile(id: "com.example.Editor", name: "Editor", settings: MouseSettings())
        XCTAssertNoThrow(try Configuration(profiles: [.global, app]).validate())
        XCTAssertThrowsError(try Configuration(profiles: [app]).validate())
        XCTAssertThrowsError(try Configuration(profiles: [.global, .global]).validate())
        XCTAssertThrowsError(try Configuration(profiles: [.global, app, app]).validate())
    }
}
