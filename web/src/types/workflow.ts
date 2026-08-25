export interface WorkflowStep {
  capability: string;
  input: 'initial' | 'merge';
  retry?: { max: number; delay_ms: number };
  compensate?: string;
}

export interface WorkflowDefinition {
  name: string;
  trigger: { event: string; subject_prefix: string };
  steps: WorkflowStep[];
  status?: 'active' | 'paused' | 'completed';
}

export interface WorkflowInstance {
  id: string;
  definitionName: string;
  currentStep?: string;
  status: 'running' | 'pending' | 'failed' | 'completed';
  createdAt: string;
  updatedAt: string;
  steps: {
    name: string;
    status: 'pending' | 'running' | 'succeeded' | 'failed';
    startedAt?: string;
    finishedAt?: string;
  }[];
}

export type WorkflowNodeType = 'trigger' | 'step' | 'action' | 'decision' | 'end';

export interface WorkflowNode {
  id: string;
  name: string;
  type: WorkflowNodeType;
}
