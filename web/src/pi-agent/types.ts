import type { WorkflowDefinition } from '../types/workflow';

export interface PiAgentConfig {
  id: string;
  name: string;
  description?: string;
  capabilities: string[];
  autoApproveThreshold?: number;
  decisionTimeoutMs?: number;
  requiredResources?: string[];
}

export interface AgentDecision {
  action: 'approve' | 'reject' | 'escalate';
  confidence: number;
  reasoning: string;
  timestamp: string;
  agentId: string;
  metadata?: Record<string, any>;
}

export interface AgentExecution {
  id: string;
  agentId: string;
  workflowStep: string;
  status: 'pending' | 'executing' | 'completed' | 'failed';
  startTime: string;
  endTime?: string;
  decision?: AgentDecision;
  error?: string;
}

export interface WorkflowWithAgents {
  definition: WorkflowDefinition;
  agents: PiAgentConfig[];
  agentDecisions: Record<string, AgentDecision[]>;
}
