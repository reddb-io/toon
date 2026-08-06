const DEFAULT_SEED = 218
const NAMES = [
  'Ada Lovelace',
  'Grace Hopper',
  'Katherine Johnson',
  'Alan Turing',
  'Margaret Hamilton',
  'Edsger Dijkstra',
  'Barbara Liskov',
  'Donald Knuth',
  'Radia Perlman',
  'Ken Thompson',
  'Frances Allen',
  'John Backus',
  'Mary Jackson',
  'Niklaus Wirth',
  'Annie Easley',
  'Claude Shannon',
  'Adele Goldberg',
  'Dennis Ritchie',
  'Jean Sammet',
  'Tony Hoare',
  'Joan Clarke',
  'Douglas Engelbart',
  'Karen Sparck Jones',
  'John McCarthy',
  'Hedy Lamarr',
  'George Boole',
  'Sophie Wilson',
]
const DEPARTMENTS = ['Engineering', 'Sales', 'Operations', 'Finance']
const STRUCTURAL_PROMPT = 'Is this data complete and structurally valid? Answer only YES or NO.'

export function createBenchmarkSuite(seed = DEFAULT_SEED) {
  const generated = generateEmployees(27, seed)
  const employees = generated.slice(0, 24)
  const structuralEmployees = generated.slice(0, 20)
  const extraEmployees = generated.slice(20, 23)
  const datasets = [
    dataset('employees', 'Generated employee records', employees),
    dataset('structural-control', 'Valid complete dataset (control)', structuralEmployees, {
      kind: 'control',
    }),
    dataset('structural-truncated', 'Three encoded rows removed from the end', structuralEmployees, {
      kind: 'truncated',
      removeRecordCount: 3,
    }),
    dataset('structural-extra-rows', 'Three encoded rows appended past the declaration', structuralEmployees, {
      kind: 'extra-rows',
      appendRecords: extraEmployees,
    }),
    dataset('structural-width-mismatch', 'Salary cell removed from encoded row ten', structuralEmployees, {
      kind: 'width-mismatch',
      targetRecordIndex: 9,
      targetFieldName: 'salary',
    }),
  ]

  return {
    version: 1,
    seed,
    datasets,
    questions: generateQuestions(employees),
  }
}

function generateEmployees(count, seed) {
  return Array.from({ length: count }, (_, index) => ({
    id: `emp-${String(index + 1).padStart(3, '0')}`,
    name: NAMES[index % NAMES.length],
    department: DEPARTMENTS[(seed + index * 3) % DEPARTMENTS.length],
    salary: 67000 + ((seed + index * 7919) % 70000),
    yearsExperience: 1 + ((seed + index * 5) % 24),
    active: (seed + index * 7) % 5 !== 3,
  }))
}

function dataset(id, description, employees, corruption) {
  return {
    id,
    description,
    data: { employees: employees.map((employee) => ({ ...employee })) },
    ...(corruption ? { corruption } : {}),
  }
}

function generateQuestions(employees) {
  const employee = (index) => employees[index]
  const count = (predicate) => employees.filter(predicate).length
  const averageSalary = employees.reduce((sum, item) => sum + item.salary, 0) / employees.length
  const structured = [
    question('employees-count', 'How many employee records are present?', employees.length, 'integer', 'structure-awareness'),
    question('employees-name-008', 'What is the name of employee emp-008?', employee(7).name, 'string', 'field-retrieval'),
    question('employees-salary-016', 'What is the salary of employee emp-016?', employee(15).salary, 'integer', 'field-retrieval'),
    question('employees-engineering-count', 'How many employees work in Engineering?', count(item => item.department === 'Engineering'), 'integer', 'aggregation'),
    question('employees-active-count', 'How many employees are active?', count(item => item.active), 'integer', 'aggregation'),
    question('employees-average-salary', 'What is the average employee salary, rounded to two decimal places?', Number(averageSalary.toFixed(2)), 'number', 'aggregation'),
    question('employees-active-high-earners', 'How many active employees have salary at least 90000?', count(item => item.active && item.salary >= 90000), 'integer', 'filtering'),
    question('employees-sales-experienced', 'How many Sales employees have more than 10 years of experience?', count(item => item.department === 'Sales' && item.yearsExperience > 10), 'integer', 'filtering'),
    question('employees-fields', 'List the employee field names in order.', Object.keys(employee(0)), 'list', 'structure-awareness'),
  ]
  const structural = [
    structuralQuestion('structural-control', true),
    structuralQuestion('structural-truncated', false),
    structuralQuestion('structural-extra-rows', false),
    structuralQuestion('structural-width-mismatch', false),
  ]
  return [...structured, ...structural]
}

function question(id, prompt, expected, answerType, category) {
  return {
    id,
    datasetId: 'employees',
    style: 'structured-question',
    category,
    prompt,
    expected,
    answerType,
  }
}

function structuralQuestion(datasetId, expected) {
  return {
    id: `${datasetId}-validity`,
    datasetId,
    style: 'structural-corruption',
    category: 'structural-validation',
    prompt: STRUCTURAL_PROMPT,
    expected: expected ? 'YES' : 'NO',
    answerType: 'boolean',
  }
}

export function encodeBenchmarkDocuments(suite, encoders) {
  assertUniqueEncoderIds(encoders)
  return suite.datasets.flatMap((entry) => encoders.map((encoder) => {
    const encoded = encoder.encode(entry.data)
    const text = entry.corruption
      ? corruptEncodedDocument(encoder, encoded, entry.corruption)
      : encoded
    return {
      datasetId: entry.id,
      encoderId: encoder.id,
      format: encoder.format,
      text,
      bytes: Buffer.byteLength(text),
    }
  }))
}

function assertUniqueEncoderIds(encoders) {
  const ids = new Set(encoders.map((encoder) => encoder.id))
  if (ids.size !== encoders.length) throw new Error('encoder ids must be unique')
  for (const encoder of encoders) {
    if (!['json', 'toon'].includes(encoder.format)) {
      throw new Error(`unsupported corruption format: ${encoder.format}`)
    }
  }
}

function corruptEncodedDocument(encoder, text, corruption) {
  if (corruption.kind === 'control') return text
  if (encoder.format === 'json') return corruptJson(text, corruption)
  return corruptToon(encoder, text, corruption)
}

function corruptJson(text, corruption) {
  const document = JSON.parse(text)
  if (corruption.kind === 'truncated') {
    document.employees = document.employees.slice(0, -corruption.removeRecordCount)
  } else if (corruption.kind === 'extra-rows') {
    document.employees.push(...corruption.appendRecords.map((record) => ({ ...record })))
  } else if (corruption.kind === 'width-mismatch') {
    delete document.employees[corruption.targetRecordIndex][corruption.targetFieldName]
  }
  return JSON.stringify(document)
}

function corruptToon(encoder, text, corruption) {
  const lines = text.split('\n')
  if (corruption.kind === 'truncated') {
    return lines.slice(0, lines.length - corruption.removeRecordCount).join('\n')
  }
  if (corruption.kind === 'extra-rows') {
    const appended = encoder.encode({ employees: corruption.appendRecords }).split('\n').slice(1)
    return [...lines, ...appended].join('\n')
  }
  const fields = headerFields(lines[0])
  const fieldIndex = fields.indexOf(corruption.targetFieldName)
  if (fieldIndex < 0) throw new Error(`TOON header has no ${corruption.targetFieldName} field`)
  const lineIndex = corruption.targetRecordIndex + 1
  const cells = lines[lineIndex].trimStart().split(',')
  cells.splice(fieldIndex, 1)
  lines[lineIndex] = `  ${cells.join(',')}`
  return lines.join('\n')
}

function headerFields(line) {
  const match = line.match(/\{([^}]*)\}/)
  if (!match) throw new Error('TOON structural fixture did not encode as a table')
  return match[1].split(',')
}
