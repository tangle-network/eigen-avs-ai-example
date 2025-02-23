// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import {TaskManager} from "./TaskManager.sol";
import {IRegistryCoordinator} from "eigenlayer-middleware/src/RegistryCoordinator.sol";

/// @title SquaringTask Contract
/// @notice A specialized TaskManager that handles squaring number tasks
contract GithubTasks is TaskManager {
    /// @notice Emitted when a squaring task is completed
    /// @notice Emitted when a squaring task is completed
    event GithubSpellingTaskCompleted(string owner, string repo, string refName);

    /// @notice Error thrown when the submitted result is incorrect
    error IncorrectSpellingResult(string owner, string repo, string refName);

    /// @notice Constructor to initialize the SquaringTask contract
    /// @param _registryCoordinator Address of the registry coordinator
    /// @param _taskResponseWindowBlock Number of blocks within which the aggregator has to respond
    constructor(
        IRegistryCoordinator _registryCoordinator,
        uint32 _taskResponseWindowBlock
    ) TaskManager(_registryCoordinator, _taskResponseWindowBlock) {}

    /// @notice Creates a new squaring task
    /// @param owner The owner of the repository
    /// @param repo The repository name
    /// @param refName The reference name of the repository
    /// @param quorumThresholdPercentage The percentage of quorum required for task completion
    /// @param quorumNumbers The quorum numbers for the task
    function createGithubSpellingTask(
        string calldata owner,
        string calldata repo,
        string calldata refName,
        uint32 quorumThresholdPercentage,
        bytes calldata quorumNumbers
    ) external onlyTaskGenerator {
        // Encode the number as the task message
        bytes memory message = abi.encode(owner, repo, refName);
        
        // Create the task using the parent contract's function
        _createNewTask(
            message,
            quorumThresholdPercentage,
            quorumNumbers
        );
    }

    /// @notice Responds to a squaring task with the computed result
    /// @param task The original task data
    /// @param taskResponse The response containing the squared result
    /// @param nonSignerStakesAndSignature Signature and stakes information for verification
    function respondToGithubSpellingTask(
        Task calldata task,
        TaskResponse calldata taskResponse,
        NonSignerStakesAndSignature memory nonSignerStakesAndSignature
    ) external onlyAggregator {
        // Call the parent contract's response function
        _respondToTask(task, taskResponse, nonSignerStakesAndSignature);

        // Decode the original number and the result
        (
            string memory owner,
            string memory repo,
            string memory refName
        ) = abi.decode(task.message, (string, string, string));

        emit GithubSpellingTaskCompleted(owner, repo, refName);
    }

    /// @notice Initializes the SquaringTask contract
    /// @param _aggregator Address of the aggregator
    /// @param _generator Address of the task generator
    /// @param initialOwner Address of the initial owner
    function initialize(
        address _aggregator,
        address _generator,
        address initialOwner
    ) external initializer {
        __TaskManager_init(_aggregator, _generator, initialOwner);
    }
}
